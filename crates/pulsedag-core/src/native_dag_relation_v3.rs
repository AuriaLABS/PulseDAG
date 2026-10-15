//! DORMANT, policy-neutral native-v3 pairwise DAG ancestry/anticone diagnostic.
//!
//! This module verifies every header on a small complete parent closure using
//! the externally trusted frozen unmined genesis and canonical KHeavyHash
//! before determining a *graph relation* between two reachable blocks.
//! Anticone here means incomparable by ancestry; it is NOT a GHOSTDAG
//! blue/red decision, K-bound classification, selected-parent/blue-work
//! authority, block admission, pruning checkpoint or mining enablement.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    errors::PulseError,
    header_v3::BlockHeaderV3,
    native_parent_dag_context_v3::{
        verify_dormant_native_v3_parent_dag, DormantV3ParentDagCheck, MAX_DORMANT_V3_DAG_HEADERS,
    },
    types::BlockId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DormantV3PairRelation {
    SameBlock,
    FirstIsAncestor,
    SecondIsAncestor,
    Anticone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantV3PairRelationCheck {
    /// Only a structural relation, never a final blue/red anticone verdict.
    pub relation: DormantV3PairRelation,
    pub verified_dag: DormantV3ParentDagCheck,
}

fn invalid(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!("dormant native-v3 relation: {}", message.into()))
}

/// Compute an inclusive past within a graph already authenticated by the
/// full-closure verifier. An independent visit bound remains defense in depth.
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
            return Err(invalid("bounded inclusive-past visit limit exceeded"));
        }
        let header = headers
            .get(&hash)
            .ok_or_else(|| invalid(format!("missing verified ancestor {hash}")))?;
        pending.extend(header.parents.iter().cloned());
    }
    Ok(seen)
}

/// Determine an ancestry relation between two blocks in the *same* verified
/// native-v3 parent-DAG closure. Both blocks must be reachable from the
/// specified canonical, unique, sorted frontier, not simply appear in the
/// untrusted header map. Side-branch PoW, complete parent links, timestamps,
/// heights and frozen genesis are checked before evaluating the relation.
///
/// `frontier` is a caller-supplied *diagnostic root set*, not an authenticated
/// selected tip or a canonical network frontier. The result has no authority
/// to classify GHOSTDAG K, select a parent, assign blue work, accept blocks,
/// or activate monetary-v3.
pub fn derive_dormant_native_v3_pair_relation(
    chain_id: &str,
    frozen_genesis_timestamp_ns: u64,
    frozen_genesis_hash: &str,
    frontier: &[BlockId],
    first: &BlockId,
    second: &BlockId,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
) -> Result<DormantV3PairRelationCheck, PulseError> {
    // This checks the entire reachable DAG first, even if first == second or
    // the comparison is otherwise easily answered by string equality.
    let verified_dag = verify_dormant_native_v3_parent_dag(
        chain_id,
        frozen_genesis_timestamp_ns,
        frozen_genesis_hash,
        frontier,
        headers,
    )?;

    let mut verified_closure = BTreeSet::new();
    for root in frontier {
        verified_closure.extend(inclusive_past(root, headers)?);
        if verified_closure.len() > MAX_DORMANT_V3_DAG_HEADERS {
            return Err(invalid("bounded verified-closure limit exceeded"));
        }
    }
    if !verified_closure.contains(first) || !verified_closure.contains(second) {
        return Err(invalid(
            "both comparison blocks must be reachable from the verified frontier",
        ));
    }

    let relation = if first == second {
        DormantV3PairRelation::SameBlock
    } else {
        let first_past = inclusive_past(first, headers)?;
        let second_past = inclusive_past(second, headers)?;
        if second_past.contains(first) {
            DormantV3PairRelation::FirstIsAncestor
        } else if first_past.contains(second) {
            DormantV3PairRelation::SecondIsAncestor
        } else {
            DormantV3PairRelation::Anticone
        }
    };

    Ok(DormantV3PairRelationCheck {
        relation,
        verified_dag,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis_v3::dormant_native_genesis_envelope_v3_ns,
        header_v3::compute_block_hash_v3,
        pow_v3::evaluate_v3_header_pow,
        protocol::BLOCK_HEADER_VERSION_V3,
        types::compute_merkle_root,
    };

    const CHAIN: &str = "dormant-native-v3-pair-relation";
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
            .expect("easy-target v3 fixture nonce exists");
        let hash = compute_block_hash_v3(&header, CHAIN).unwrap();
        headers.insert(hash.clone(), header);
        hash
    }

    fn relation(
        genesis: &str,
        frontier: &[BlockId],
        first: &BlockId,
        second: &BlockId,
        headers: &BTreeMap<BlockId, BlockHeaderV3>,
    ) -> Result<DormantV3PairRelation, PulseError> {
        Ok(derive_dormant_native_v3_pair_relation(
            CHAIN, GENESIS_NS, genesis, frontier, first, second, headers,
        )?
        .relation)
    }

    #[test]
    fn fork_join_is_anticone_but_merge_descends_from_both_parents() {
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
        let merged = append(
            &mut headers,
            vec![a.clone(), b.clone()],
            2,
            GENESIS_NS + 1_000_000_000,
            true,
        );
        let tips = vec![merged.clone()];
        assert_eq!(
            relation(&genesis, &tips, &a, &b, &headers).unwrap(),
            DormantV3PairRelation::Anticone
        );
        assert_eq!(
            relation(&genesis, &tips, &b, &a, &headers).unwrap(),
            DormantV3PairRelation::Anticone
        );
        assert_eq!(
            relation(&genesis, &tips, &a, &merged, &headers).unwrap(),
            DormantV3PairRelation::FirstIsAncestor
        );
        assert_eq!(
            relation(&genesis, &tips, &merged, &b, &headers).unwrap(),
            DormantV3PairRelation::SecondIsAncestor
        );
        assert_eq!(
            relation(&genesis, &tips, &merged, &merged, &headers).unwrap(),
            DormantV3PairRelation::SameBlock
        );
        assert_eq!(
            relation(&genesis, &tips, &genesis, &b, &headers).unwrap(),
            DormantV3PairRelation::FirstIsAncestor
        );
    }

    #[test]
    fn two_parallel_frontier_tips_are_incomparable() {
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
        let mut frontier = vec![a.clone(), b.clone()];
        frontier.sort();
        let check = derive_dormant_native_v3_pair_relation(
            CHAIN, GENESIS_NS, &genesis, &frontier, &a, &b, &headers,
        )
        .unwrap();
        assert_eq!(check.relation, DormantV3PairRelation::Anticone);
        assert_eq!(check.verified_dag.authenticated_non_genesis_headers, 2);
    }

    #[test]
    fn cannot_query_unreachable_extra_header_as_authenticated() {
        let (genesis, mut headers) = fixture();
        let a = append(
            &mut headers,
            vec![genesis.clone()],
            1,
            GENESIS_NS + 500_000_000,
            true,
        );
        let extra = append(
            &mut headers,
            vec![genesis.clone()],
            1,
            GENESIS_NS + 600_000_000,
            true,
        );
        let err = relation(
            &genesis, std::slice::from_ref(&a), &a, &extra, &headers,
        )
        .unwrap_err();
        assert!(err.to_string().contains("reachable from the verified frontier"));
    }

    #[test]
    fn rejects_invalid_side_pow_even_for_same_block_query() {
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
        let merge = append(
            &mut headers,
            vec![a.clone(), bad],
            2,
            GENESIS_NS + 1_000_000_000,
            true,
        );
        let err = relation(
            &genesis, &[merge], &a, &a, &headers,
        )
        .unwrap_err();
        assert!(err.to_string().contains("invalid PoW"));
    }

    #[test]
    fn rejects_fake_identity_missing_ancestor_or_unsorted_frontier() {
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
        assert!(relation("00", std::slice::from_ref(&a), &a, &a, &headers).is_err());
        let mut backwards = vec![a.clone(), b.clone()];
        backwards.sort();
        backwards.reverse();
        assert!(relation(&genesis, &backwards, &a, &b, &headers).is_err());
        headers.remove(&genesis);
        assert!(relation(&genesis, &[a], &b, &b, &headers).is_err());
    }
}
