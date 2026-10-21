//! DORMANT native-v3 all-vertex anticone diagnostic, with no consensus K.
//! Graph-theoretic incomparability is not a GHOSTDAG blue/red decision.
//! Only complete, genesis-bound KHeavyHash-authenticated ancestor closures
//! are accepted. No mining/P2P/replay/contract activation hooks exist here.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    errors::PulseError,
    header_v3::BlockHeaderV3,
    native_parent_dag_context_v3::MAX_DORMANT_V3_DAG_HEADERS,
    native_topological_evaluation_v3::{
        derive_dormant_native_v3_topological_evaluation, DormantV3TopologicalEvaluation,
    },
    types::BlockId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantV3AnticoneMatrix {
    /// Lexicographically ordered anticone for each reachable block.
    /// Excludes the block itself and all its ancestors/descendants.
    pub anticone_by_block: BTreeMap<BlockId, Vec<BlockId>>,
    /// Verified *evaluation* order, not consensus transaction/blue order.
    pub topological_evaluation: DormantV3TopologicalEvaluation,
}

fn invalid(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!("dormant v3 anticone matrix: {}", message.into()))
}

/// Compute the all-block incomparability table after a single complete
/// authenticated topology pass. Local O(N²) ancestor caches are capped at
/// 128 headers and MUST NOT become active-v3 consensus budgets by accident.
pub fn derive_dormant_native_v3_anticone_matrix(
    chain_id: &str,
    frozen_genesis_timestamp_ns: u64,
    frozen_genesis_hash: &str,
    frontier: &[BlockId],
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
) -> Result<DormantV3AnticoneMatrix, PulseError> {
    let evaluation = derive_dormant_native_v3_topological_evaluation(
        chain_id,
        frozen_genesis_timestamp_ns,
        frozen_genesis_hash,
        frontier,
        headers,
    )?;
    let all = &evaluation.ordered_reachable_headers;

    // Each parent is already processed in authenticated topological order.
    let mut pasts: BTreeMap<BlockId, BTreeSet<BlockId>> = BTreeMap::new();
    for hash in all {
        let header = headers
            .get(hash)
            .ok_or_else(|| invalid(format!("missing authenticated header {hash}")))?;
        let mut past = BTreeSet::new();
        past.insert(hash.clone());
        for parent in &header.parents {
            let parent_past = pasts
                .get(parent)
                .ok_or_else(|| invalid(format!("missing predecessor {parent}")))?;
            past.extend(parent_past.iter().cloned());
        }
        if past.len() > MAX_DORMANT_V3_DAG_HEADERS {
            return Err(invalid("bounded ancestor past overflow"));
        }
        pasts.insert(hash.clone(), past);
    }

    let mut anticone_by_block = BTreeMap::new();
    for first in all {
        let first_past = pasts
            .get(first)
            .ok_or_else(|| invalid(format!("missing first past {first}")))?;
        let mut incomparable = BTreeSet::new();
        for second in all {
            let second_past = pasts
                .get(second)
                .ok_or_else(|| invalid(format!("missing second past {second}")))?;
            if !first_past.contains(second) && !second_past.contains(first) {
                incomparable.insert(second.clone());
            }
        }
        anticone_by_block.insert(first.clone(), incomparable.into_iter().collect());
    }

    Ok(DormantV3AnticoneMatrix {
        anticone_by_block,
        topological_evaluation: evaluation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis_v3::dormant_native_genesis_envelope_v3_ns,
        header_v3::compute_block_hash_v3,
        native_dag_relation_v3::{derive_dormant_native_v3_pair_relation, DormantV3PairRelation},
        pow_v3::evaluate_v3_header_pow,
        protocol::BLOCK_HEADER_VERSION_V3,
        types::compute_merkle_root,
    };

    const CHAIN: &str = "dormant-native-v3-anticone-matrix";
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
            .expect("easy target test nonce exists");
        let hash = compute_block_hash_v3(&header, CHAIN).unwrap();
        headers.insert(hash.clone(), header);
        hash
    }

    fn matrix(
        genesis: &str,
        frontier: &[BlockId],
        headers: &BTreeMap<BlockId, BlockHeaderV3>,
    ) -> Result<DormantV3AnticoneMatrix, PulseError> {
        derive_dormant_native_v3_anticone_matrix(CHAIN, GENESIS_NS, genesis, frontier, headers)
    }

    #[test]
    fn compares_all_pairs_to_existing_independent_single_pair_reference() {
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
        let a_tip = append(&mut headers, vec![a], 2, GENESIS_NS + 1_100_000_000, true);
        let mut frontier = vec![joined, a_tip];
        frontier.sort();
        let result = matrix(&genesis, &frontier, &headers).unwrap();
        assert_eq!(result.anticone_by_block.len(), 5);
        for (first, anticone) in &result.anticone_by_block {
            for second in result.anticone_by_block.keys() {
                let expected = derive_dormant_native_v3_pair_relation(
                    CHAIN, GENESIS_NS, &genesis, &frontier, first, second, &headers,
                )
                .unwrap()
                .relation
                    == DormantV3PairRelation::Anticone;
                assert_eq!(anticone.contains(second), expected);
                assert_eq!(
                    anticone.contains(second),
                    result.anticone_by_block[second].contains(first)
                );
            }
        }
        assert!(result.anticone_by_block[&genesis].is_empty());
    }

    #[test]
    fn ignores_unreachable_bad_index_and_rebuild_order() {
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
        let mut frontier = vec![a, b];
        frontier.sort();
        let original = matrix(&genesis, &frontier, &headers).unwrap();
        let mut unrelated = headers[&genesis].clone();
        unrelated.nonce += 30;
        headers.insert("ff".repeat(32), unrelated);
        let rebuilt: BTreeMap<_, _> = headers.into_iter().rev().collect();
        assert_eq!(original, matrix(&genesis, &frontier, &rebuilt).unwrap());
    }

    #[test]
    fn bad_side_pow_or_missing_ancestor_denies_entire_matrix() {
        let (genesis, mut headers) = fixture();
        let good = append(
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
        let mut frontier = vec![good, bad.clone()];
        frontier.sort();
        assert!(matrix(&genesis, &frontier, &headers)
            .unwrap_err()
            .to_string()
            .contains("invalid PoW"));
        headers.remove(&bad);
        assert!(matrix(&genesis, &frontier, &headers)
            .unwrap_err()
            .to_string()
            .contains("missing ancestor header"));
    }

    #[test]
    fn wrong_genesis_unsorted_frontier_and_cross_chain_fail_closed() {
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
        let mut frontier = vec![a, b];
        frontier.sort();
        assert!(matrix("00", &frontier, &headers).is_err());
        frontier.reverse();
        assert!(matrix(&genesis, &frontier, &headers).is_err());
        // Restore a canonical frontier so this rejection is specifically
        // attributable to chain-domain authentication, not parent ordering.
        frontier.sort();
        assert!(derive_dormant_native_v3_anticone_matrix(
            "different-chain",
            GENESIS_NS,
            &genesis,
            &frontier,
            &headers,
        )
        .is_err());
    }
}
