//! DORMANT, K-neutral topological evaluation of an authenticated native-v3 DAG.
//!
//! Every returned header belongs to the COMPLETE ancestor closure of the
//! caller's candidate parents, verified back to an externally frozen genesis.
//! The diagnostic order is (validated height, canonical hash), so a parent
//! always precedes each child and arrival order cannot influence the result.
//!
//! IMPORTANT: this is an *evaluation order*, NOT consensus-selected parent,
//! GHOSTDAG blue/red order, canonical transaction order, or cumulative work.
//! It must not authorize mined/P2P admission, reward replay or v3 mining.

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
pub struct DormantV3TopologicalEvaluation {
    /// Local deterministic *topological* diagnostic, not GHOSTDAG block order.
    pub ordered_reachable_headers: Vec<BlockId>,
    pub verified_dag: DormantV3ParentDagCheck,
}

fn invalid(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "dormant native v3 topological evaluation: {}",
        message.into()
    ))
}

/// Validate every reachable ancestor before deriving a fixed topological walk.
///
/// Authentication includes KHeavyHash PoW on ALL reachable non-genesis headers,
/// including unselected side branches; missing/pruned ancestry fails closed.
/// Extra map entries not reachable from the requested roots are ignored.
///
/// Height/hash sorting is deliberately NOT an adopted native-v3 GHOSTDAG tie
/// policy, K, work metric, blue/red ordering or admission decision.
pub fn derive_dormant_native_v3_topological_evaluation(
    chain_id: &str,
    frozen_genesis_timestamp_ns: u64,
    frozen_genesis_hash: &str,
    candidate_parents: &[BlockId],
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
) -> Result<DormantV3TopologicalEvaluation, PulseError> {
    let verified_dag = verify_dormant_native_v3_parent_dag(
        chain_id,
        frozen_genesis_timestamp_ns,
        frozen_genesis_hash,
        candidate_parents,
        headers,
    )?;

    let mut seen = BTreeSet::new();
    let mut pending = candidate_parents.to_vec();
    while let Some(hash) = pending.pop() {
        if !seen.insert(hash.clone()) {
            continue;
        }
        if seen.len() > MAX_DORMANT_V3_DAG_HEADERS {
            return Err(invalid("reachable header budget exceeded"));
        }
        let header = headers
            .get(&hash)
            .ok_or_else(|| invalid(format!("missing authenticated header {hash}")))?;
        pending.extend(header.parents.iter().cloned());
    }

    // Sorting by VERIFIED DAG height followed by canonical identifier yields
    // a deterministic topological order without inventing a selected parent.
    let mut keyed = Vec::with_capacity(seen.len());
    for hash in &seen {
        let header = headers
            .get(hash)
            .ok_or_else(|| invalid(format!("missing authenticated header {hash}")))?;
        keyed.push((header.height, hash.clone()));
    }
    keyed.sort_unstable();
    let ordered_reachable_headers: Vec<BlockId> = keyed.into_iter().map(|(_, hash)| hash).collect();

    if ordered_reachable_headers.first().map(String::as_str) != Some(frozen_genesis_hash)
        || ordered_reachable_headers.len() != verified_dag.authenticated_non_genesis_headers + 1
    {
        return Err(invalid("authenticated ancestor closure/count disagreement"));
    }
    let positions: BTreeMap<&BlockId, usize> = ordered_reachable_headers
        .iter()
        .enumerate()
        .map(|(index, hash)| (hash, index))
        .collect();
    for (child_position, hash) in ordered_reachable_headers.iter().enumerate() {
        let header = headers
            .get(hash)
            .ok_or_else(|| invalid(format!("missing authenticated header {hash}")))?;
        for parent in &header.parents {
            match positions.get(parent) {
                Some(parent_position) if *parent_position < child_position => {}
                _ => return Err(invalid("topological parent-before-child invariant failed")),
            }
        }
    }

    Ok(DormantV3TopologicalEvaluation {
        ordered_reachable_headers,
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

    const CHAIN: &str = "dormant-native-v3-topological-order";
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
            .expect("easy PoW fixture nonce exists");
        let hash = compute_block_hash_v3(&header, CHAIN).unwrap();
        headers.insert(hash.clone(), header);
        hash
    }

    fn evaluate(
        genesis: &str,
        parents: &[BlockId],
        headers: &BTreeMap<BlockId, BlockHeaderV3>,
    ) -> Result<DormantV3TopologicalEvaluation, PulseError> {
        derive_dormant_native_v3_topological_evaluation(
            CHAIN, GENESIS_NS, genesis, parents, headers,
        )
    }

    #[test]
    fn branch_join_order_is_height_then_hash_and_parent_before_child() {
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
        let side = append(
            &mut headers,
            vec![b.clone()],
            2,
            GENESIS_NS + 1_200_000_000,
            true,
        );
        let tip = append(
            &mut headers,
            vec![joined.clone(), side.clone()],
            3,
            GENESIS_NS + 1_800_000_000,
            true,
        );
        let found = evaluate(&genesis, &[tip.clone()], &headers).unwrap();
        let mut height_one = vec![a, b];
        height_one.sort();
        let mut height_two = vec![joined, side];
        height_two.sort();
        let mut expected = vec![genesis];
        expected.extend(height_one);
        expected.extend(height_two);
        expected.push(tip);
        assert_eq!(found.ordered_reachable_headers, expected);
        assert_eq!(found.verified_dag.authenticated_non_genesis_headers, 5);
        assert_eq!(found.verified_dag.authenticated_parent_edges, 7);
    }

    #[test]
    fn diagnostic_does_not_depend_on_candidate_order_or_unreachable_index_entries() {
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
        let mut roots = vec![a, b];
        roots.sort();
        let original = evaluate(&genesis, &roots, &headers).unwrap();

        // An unqueried malformed entry must not contaminate a valid closure.
        let mut bad = headers.get(&genesis).unwrap().clone();
        bad.nonce += 42;
        headers.insert("ff".repeat(32), bad);
        let after_extra = evaluate(&genesis, &roots, &headers).unwrap();
        assert_eq!(original, after_extra);

        // BTreeMap contents constructed in reversed insertion order produce
        // the same authenticated closure and deterministic diagnostic order.
        let mut reversed = BTreeMap::new();
        for (hash, header) in headers.iter().rev() {
            reversed.insert(hash.clone(), header.clone());
        }
        assert_eq!(original, evaluate(&genesis, &roots, &reversed).unwrap());
    }

    #[test]
    fn rejects_missing_side_ancestry_and_side_branch_pow() {
        let (genesis, mut headers) = fixture();
        let a = append(
            &mut headers,
            vec![genesis.clone()],
            1,
            GENESIS_NS + 500_000_000,
            true,
        );
        let bad = append(
            &mut headers,
            vec![genesis.clone()],
            1,
            GENESIS_NS + 600_000_000,
            false,
        );
        let mut roots = vec![a.clone(), bad.clone()];
        roots.sort();
        let error = evaluate(&genesis, &roots, &headers).unwrap_err();
        assert!(error.to_string().contains("invalid PoW"));

        headers.remove(&bad);
        let missing = evaluate(&genesis, &roots, &headers).unwrap_err();
        assert!(missing.to_string().contains("missing ancestor header"));

        let valid = evaluate(&genesis, &[a], &headers).unwrap();
        assert_eq!(valid.ordered_reachable_headers.len(), 2);
    }

    #[test]
    fn fails_closed_for_untrusted_genesis_unsorted_roots_and_other_chain() {
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
        let mut unsorted = vec![a.clone(), b.clone()];
        unsorted.sort();
        unsorted.reverse();
        assert!(evaluate(&genesis, &unsorted, &headers).is_err());
        assert!(evaluate("00", &[a.clone()], &headers).is_err());
        assert!(derive_dormant_native_v3_topological_evaluation(
            "different-chain",
            GENESIS_NS,
            &genesis,
            &[a],
            &headers,
        )
        .is_err());
    }
}
