//! DORMANT native-v3 bounded parent-DAG closure verifier.
//!
//! This independently verifies native nanosecond header/hash/KHeavyHash and
//! complete parent linkage from each supplied direct parent down to the exact
//! externally frozen, non-mined genesis. It accepts a *small complete* DAG,
//! never an untrusted/pruned historical boundary. The result is diagnostics,
//! NOT selected-parent or blue-work authority, consensus acceptance, a
//! replay/storage checkpoint, or a permission to start mining/P2P.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    errors::PulseError,
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::{compute_block_hash_v3, BlockHeaderV3},
    pow_v3::verify_v3_header_pow,
    selection_v2::GHOSTDAG_V1_MAX_PARENTS,
    types::BlockId,
};

/// Local diagnostic CPU/space caps, not network consensus limits.
pub const MAX_DORMANT_V3_DAG_HEADERS: usize = 128;
pub const MAX_DORMANT_V3_DAG_EDGES: usize = 2_048;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantV3ParentDagCheck {
    pub direct_parents: usize,
    pub authenticated_non_genesis_headers: usize,
    pub authenticated_parent_edges: usize,
    pub frozen_genesis_reached: bool,
}

fn invalid(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!("dormant v3 parent DAG: {}", message.into()))
}

/// Verify *all* ancestors reachable from the supplied immediate parent set.
///
/// The freeze inputs MUST come from an independent trusted network identity,
/// not the supplied DAG. No caller-provided selected-parent, blue-work, or
/// blue-score map is consulted. This intentionally refuses incomplete/pruned
/// history, even if the missing parent is older than a retarget window.
///
/// The returned check is not an admission token. In particular this function
/// does NOT choose the selected parent, classify blue/red merge sets, derive
/// cumulative blue-work, validate candidate transactions/state, or modify DB.
pub fn verify_dormant_native_v3_parent_dag(
    chain_id: &str,
    frozen_genesis_timestamp_ns: u64,
    frozen_genesis_hash: &str,
    direct_parents: &[BlockId],
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
) -> Result<DormantV3ParentDagCheck, PulseError> {
    if direct_parents.is_empty() || direct_parents.len() > GHOSTDAG_V1_MAX_PARENTS {
        return Err(invalid("direct-parent count outside 1..=64"));
    }
    if direct_parents.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(invalid("direct parents must be unique and canonically sorted"));
    }

    let genesis = dormant_native_genesis_envelope_v3_ns(chain_id, frozen_genesis_timestamp_ns)
        .map_err(|error| invalid(format!("frozen genesis inputs: {error}")))?;
    if genesis.hash != frozen_genesis_hash {
        return Err(invalid("trusted frozen genesis hash differs from canonical identity"));
    }

    let mut pending: Vec<BlockId> = direct_parents.to_vec();
    let mut visited = BTreeSet::new();
    let mut edge_count = 0usize;
    let mut non_genesis = 0usize;
    let mut genesis_seen = false;

    while let Some(hash) = pending.pop() {
        if !visited.insert(hash.clone()) {
            continue;
        }
        if visited.len() > MAX_DORMANT_V3_DAG_HEADERS {
            return Err(invalid("bounded parent-DAG header visit limit exceeded"));
        }
        let header = headers
            .get(&hash)
            .ok_or_else(|| invalid(format!("missing ancestor header {hash}")))?;
        let computed = compute_block_hash_v3(header, chain_id)
            .map_err(|error| invalid(format!("invalid native-v3 header {hash}: {error}")))?;
        if computed != hash {
            return Err(invalid(format!("ancestor header key/hash mismatch for {hash}")));
        }

        if header.height == 0 {
            if hash != genesis.hash || header != &genesis.header {
                return Err(invalid("height-zero ancestor is not the exact frozen genesis"));
            }
            genesis_seen = true;
            continue;
        }

        // A valid chain-bound SHA-256 header hash cannot substitute for
        // the full-width canonical KHeavyHash proof of work.
        verify_v3_header_pow(header, chain_id)
            .map_err(|error| invalid(format!("ancestor {hash} has invalid PoW: {error}")))?;
        non_genesis += 1;
        edge_count = edge_count
            .checked_add(header.parents.len())
            .ok_or_else(|| invalid("parent-edge counter overflow"))?;
        if edge_count > MAX_DORMANT_V3_DAG_EDGES {
            return Err(invalid("bounded parent-DAG edge limit exceeded"));
        }

        let mut max_height = 0u64;
        let mut max_timestamp_ns = 0u64;
        for parent in &header.parents {
            let parent_header = headers
                .get(parent)
                .ok_or_else(|| invalid(format!("missing ancestor parent {parent}")))?;
            max_height = max_height.max(parent_header.height);
            max_timestamp_ns = max_timestamp_ns.max(parent_header.timestamp_ns);
            if parent_header.height >= header.height {
                return Err(invalid(format!("ancestor {hash} has non-lower parent {parent}")));
            }
            if parent_header.timestamp_ns >= header.timestamp_ns {
                return Err(invalid(format!("ancestor {hash} has non-earlier parent {parent}")));
            }
            pending.push(parent.clone());
        }
        let expected_height = max_height
            .checked_add(1)
            .ok_or_else(|| invalid("ancestor height overflow"))?;
        if header.height != expected_height || header.timestamp_ns <= max_timestamp_ns {
            return Err(invalid(format!("ancestor {hash} has invalid DAG height/timestamp")));
        }
    }

    if !genesis_seen {
        return Err(invalid("complete parent-DAG closure lacks frozen genesis"));
    }
    Ok(DormantV3ParentDagCheck {
        direct_parents: direct_parents.len(),
        authenticated_non_genesis_headers: non_genesis,
        authenticated_parent_edges: edge_count,
        frozen_genesis_reached: true,
    })
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

    const CHAIN: &str = "dormant-native-v3-parent-dag";
    const GENESIS_NS: u64 = 1_800_000_000_500_000_000;
    const EASY_BITS: u32 = 0x207f_ffff;

    fn fixture() -> (BlockEnvelopeV3, BTreeMap<BlockId, BlockHeaderV3>) {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let mut headers = BTreeMap::new();
        headers.insert(genesis.hash.clone(), genesis.header.clone());
        (genesis, headers)
    }

    fn append_header(
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
            .expect("easy-target v3 test fixture nonce exists");
        let hash = compute_block_hash_v3(&header, CHAIN).unwrap();
        headers.insert(hash.clone(), header);
        hash
    }

    #[test]
    fn independently_verifies_both_fork_ancestors_in_a_merge_dag() {
        let (genesis, mut headers) = fixture();
        let a = append_header(
            &mut headers, vec![genesis.hash.clone()], 1,
            GENESIS_NS + 500_000_000, true,
        );
        let b = append_header(
            &mut headers, vec![genesis.hash.clone()], 1,
            GENESIS_NS + 600_000_000, true,
        );
        let merge = append_header(
            &mut headers, vec![a.clone(), b.clone()], 2,
            GENESIS_NS + 1_000_000_000, true,
        );
        let result = verify_dormant_native_v3_parent_dag(
            CHAIN, GENESIS_NS, &genesis.hash, &[merge], &headers,
        )
        .unwrap();
        assert_eq!(result.authenticated_non_genesis_headers, 3);
        assert_eq!(result.authenticated_parent_edges, 4);
        assert!(result.frozen_genesis_reached);
    }

    #[test]
    fn rejects_invalid_pow_in_a_side_branch_even_when_hash_is_consistent() {
        let (genesis, mut headers) = fixture();
        let good = append_header(
            &mut headers, vec![genesis.hash.clone()], 1,
            GENESIS_NS + 500_000_000, true,
        );
        let bad = append_header(
            &mut headers, vec![genesis.hash.clone()], 1,
            GENESIS_NS + 600_000_000, false,
        );
        let merge = append_header(
            &mut headers, vec![good, bad], 2,
            GENESIS_NS + 1_000_000_000, true,
        );
        let error = verify_dormant_native_v3_parent_dag(
            CHAIN, GENESIS_NS, &genesis.hash, &[merge], &headers,
        )
        .unwrap_err();
        assert!(error.to_string().contains("invalid PoW"));
    }

    #[test]
    fn rejects_rehashed_unfrozen_genesis_and_missing_side_history() {
        let (genesis, mut headers) = fixture();
        let mut fake = genesis.header.clone();
        fake.nonce += 1;
        let fake_hash = compute_block_hash_v3(&fake, CHAIN).unwrap();
        headers.insert(fake_hash.clone(), fake);
        let wrong_root_child = append_header(
            &mut headers, vec![fake_hash], 1, GENESIS_NS + 500_000_000, true,
        );
        let error = verify_dormant_native_v3_parent_dag(
            CHAIN, GENESIS_NS, &genesis.hash, &[wrong_root_child], &headers,
        )
        .unwrap_err();
        assert!(error.to_string().contains("exact frozen genesis"));

        let honest_child = append_header(
            &mut headers, vec![genesis.hash.clone()], 1,
            GENESIS_NS + 500_000_000, true,
        );
        headers.remove(&genesis.hash);
        let error = verify_dormant_native_v3_parent_dag(
            CHAIN, GENESIS_NS, &genesis.hash, &[honest_child], &headers,
        )
        .unwrap_err();
        assert!(error.to_string().contains("missing ancestor parent"));
    }

    #[test]
    fn rejects_wrong_height_and_nonmonotonic_nanoseconds() {
        let (genesis, mut headers) = fixture();
        let too_high = append_header(
            &mut headers, vec![genesis.hash.clone()], 3,
            GENESIS_NS + 500_000_000, true,
        );
        let error = verify_dormant_native_v3_parent_dag(
            CHAIN, GENESIS_NS, &genesis.hash, &[too_high], &headers,
        )
        .unwrap_err();
        assert!(error.to_string().contains("invalid DAG height"));

        let stale = append_header(
            &mut headers, vec![genesis.hash.clone()], 1,
            GENESIS_NS, true,
        );
        let error = verify_dormant_native_v3_parent_dag(
            CHAIN, GENESIS_NS, &genesis.hash, &[stale], &headers,
        )
        .unwrap_err();
        assert!(error.to_string().contains("non-earlier parent"));
    }

    #[test]
    fn rejects_untrusted_frozen_hash_and_incomplete_parent_set() {
        let (genesis, mut headers) = fixture();
        let tip = append_header(
            &mut headers, vec![genesis.hash.clone()], 1,
            GENESIS_NS + 500_000_000, true,
        );
        assert!(verify_dormant_native_v3_parent_dag(
            CHAIN, GENESIS_NS, "00", &[tip.clone()], &headers,
        )
        .is_err());
        assert!(verify_dormant_native_v3_parent_dag(
            CHAIN, GENESIS_NS, &genesis.hash, &[], &headers,
        )
        .is_err());
        assert!(verify_dormant_native_v3_parent_dag(
            CHAIN, GENESIS_NS, &genesis.hash, &[tip.clone(), tip], &headers,
        )
        .is_err());
    }

    #[test]
    fn rejects_unbounded_complete_history_instead_of_trusting_pruned_input() {
        let (genesis, mut headers) = fixture();
        let mut tip = genesis.hash.clone();
        for height in 1..=MAX_DORMANT_V3_DAG_HEADERS as u64 {
            tip = append_header(
                &mut headers, vec![tip], height,
                GENESIS_NS + height * 500_000_000, true,
            );
        }
        let over_limit = append_header(
            &mut headers, vec![tip], MAX_DORMANT_V3_DAG_HEADERS as u64 + 1,
            GENESIS_NS + (MAX_DORMANT_V3_DAG_HEADERS as u64 + 1) * 500_000_000,
            true,
        );
        let error = verify_dormant_native_v3_parent_dag(
            CHAIN, GENESIS_NS, &genesis.hash, &[over_limit], &headers,
        )
        .unwrap_err();
        assert!(error.to_string().contains("visit limit"));
    }
}
