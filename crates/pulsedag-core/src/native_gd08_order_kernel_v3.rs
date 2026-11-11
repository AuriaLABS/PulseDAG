//! Native-v3 GD-08 **dormant ordering kernel** for authenticated, complete DAGs.
//!
//! Unlike height/hash-only topological diagnostics, this kernel enforces a
//! supplied selected-spine/merge-anchor inclusion *contract* before deriving
//! a deterministic parent-before-child order. It reauthenticates every block
//! from frozen genesis and rejects missing or mis-anchored side history.
//!
//! The caller's spine and blue/red classifications remain UNAPPROVED inputs,
//! not consensus authority. This module does NOT accept money, compute rewards,
//! activate mining, authorize pruning, or establish a final v3 policy (#1410).

use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};

use crate::{
    errors::PulseError, header_v3::BlockHeaderV3,
    native_ghostdag_policy_candidate_v3::derive_dormant_native_v3_ghostdag_trial,
    native_topological_evaluation_v3::derive_dormant_native_v3_topological_evaluation,
    types::BlockId,
};

const DIGEST_DOMAIN: &[u8] = b"PulseDAG:UNAPPROVED:GD08:authenticated-order-kernel:v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DormantGd08Role {
    MergeBlue,
    MergeRed,
    Selected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DormantGd08AnchorSlot {
    /// Selected-spine index, beginning with frozen genesis at index 0.
    pub anchor: usize,
    pub role: DormantGd08Role,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantGd08OrderPreview {
    pub ordered: Vec<BlockId>,
    pub digest: String,
    pub blue_count: usize,
    pub red_count: usize,
}

fn reject(why: &str) -> PulseError {
    PulseError::InvalidBlock(format!("UNAPPROVED native GD-08 order: {why}"))
}

fn ancestor_or_self(
    ancestor: &str,
    descendant: &str,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
) -> bool {
    let mut pending = vec![descendant.to_string()];
    let mut visited = BTreeSet::new();
    while let Some(next) = pending.pop() {
        if !visited.insert(next.clone()) {
            continue;
        }
        if next == ancestor {
            return true;
        }
        if let Some(header) = headers.get(&next) {
            pending.extend(header.parents.iter().cloned());
        }
    }
    false
}

/// Enforce complete PoW-authenticated ancestry and a self-consistent, unique
/// selected-chain/merge-anchor assignment, then produce a deterministic order.
///
/// The caller-supplied roles and selected chain are NOT independently derived
/// GHOSTDAG selected-parent/blue-work or GD-08 monetary eligibility. Never
/// treat this return value as UTXO/reward authority before #1309/#1410 freeze.
pub fn derive_dormant_native_gd08_order(
    chain_id: &str,
    frozen_genesis_timestamp_ns: u64,
    frozen_genesis_hash: &str,
    selected_spine: &[BlockId],
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
    assignments: &BTreeMap<BlockId, DormantGd08AnchorSlot>,
) -> Result<DormantGd08OrderPreview, PulseError> {
    if selected_spine.is_empty()
        || selected_spine.first().map(String::as_str) != Some(frozen_genesis_hash)
    {
        return Err(reject("selected spine must start at frozen genesis"));
    }
    let tip = selected_spine
        .last()
        .ok_or_else(|| reject("missing selected tip"))?;
    let verified = derive_dormant_native_v3_topological_evaluation(
        chain_id,
        frozen_genesis_timestamp_ns,
        frozen_genesis_hash,
        std::slice::from_ref(tip),
        headers,
    )?;
    let reachable: BTreeSet<BlockId> = verified.ordered_reachable_headers.iter().cloned().collect();
    let selected: BTreeSet<BlockId> = selected_spine.iter().cloned().collect();
    if selected.len() != selected_spine.len() {
        return Err(reject("duplicate selected-spine header"));
    }
    if reachable.len() != assignments.len()
        || reachable.iter().any(|hash| !assignments.contains_key(hash))
        || assignments.keys().any(|hash| !reachable.contains(hash))
    {
        return Err(reject(
            "missing, extra or duplicate role/anchor assignments",
        ));
    }
    for (index, hash) in selected_spine.iter().enumerate() {
        if !reachable.contains(hash) {
            return Err(reject("selected spine references unverified header"));
        }
        if assignments.get(hash)
            != Some(&DormantGd08AnchorSlot {
                anchor: index,
                role: DormantGd08Role::Selected,
            })
        {
            return Err(reject("selected-spine slot/role mismatch"));
        }
        if index > 0 {
            let header = headers
                .get(hash)
                .ok_or_else(|| reject("missing spine header"))?;
            if !header.parents.contains(&selected_spine[index - 1]) {
                return Err(reject(
                    "claimed selected parent is not a direct header parent",
                ));
            }
        }
    }
    for (hash, slot) in assignments {
        if selected.contains(hash) {
            continue;
        }
        if slot.role == DormantGd08Role::Selected
            || slot.anchor == 0
            || slot.anchor >= selected_spine.len()
        {
            return Err(reject("side header claims invalid selected role or anchor"));
        }
        let first_reachable_anchor = selected_spine
            .iter()
            .position(|spine_hash| ancestor_or_self(hash, spine_hash, headers));
        if first_reachable_anchor != Some(slot.anchor) {
            return Err(reject("side header assigned to non-earliest merge anchor"));
        }
    }

    let mut remaining = reachable;
    let mut emitted = BTreeSet::new();
    let mut ordered = Vec::with_capacity(remaining.len());
    while !remaining.is_empty() {
        let chosen = remaining
            .iter()
            .filter(|hash| {
                headers[*hash]
                    .parents
                    .iter()
                    .all(|parent| emitted.contains(parent))
            })
            .min_by_key(|hash| {
                let slot = assignments[*hash];
                (slot.anchor, slot.role, (*hash).clone())
            })
            .ok_or_else(|| reject("incomplete or cyclic parent-before-child order"))?
            .clone();
        remaining.remove(&chosen);
        emitted.insert(chosen.clone());
        ordered.push(chosen);
    }
    if ordered.first().map(String::as_str) != Some(frozen_genesis_hash)
        || ordered.last().map(String::as_str) != Some(tip.as_str())
    {
        return Err(reject("selected genesis/tip order mismatch"));
    }
    let mut hasher = Sha256::new();
    hasher.update(DIGEST_DOMAIN);
    hasher.update((chain_id.len() as u64).to_le_bytes());
    hasher.update(chain_id.as_bytes());
    hasher.update((selected_spine.len() as u64).to_le_bytes());
    for hash in &ordered {
        let slot = assignments[hash];
        hasher.update((hash.len() as u64).to_le_bytes());
        hasher.update(hash.as_bytes());
        hasher.update((slot.anchor as u64).to_le_bytes());
        hasher.update([slot.role as u8]);
    }
    Ok(DormantGd08OrderPreview {
        blue_count: assignments
            .values()
            .filter(|s| s.role == DormantGd08Role::MergeBlue)
            .count(),
        red_count: assignments
            .values()
            .filter(|s| s.role == DormantGd08Role::MergeRed)
            .count(),
        ordered,
        digest: hex::encode(hasher.finalize()),
    })
}

/// Reconstruct the GD-08 diagnostic roles from authenticated GHOSTDAG trial
/// metadata instead of trusting caller-supplied selected-spine/anchor/color
/// assignments. This closes a *diagnostic integration* gap between GD-01..07
/// and GD-08 (#1309/#1410); it does not freeze K or blue/red monetary policy.
///
/// Every ancestor, PoW, committed blue score and proposed K-cluster is derived
/// from exact native-v3 headers. Each side block must be classified exactly
/// once at its selected-spine merge anchor; missing/duplicate slots fail closed.
/// The returned order is NOT a spending, settlement, mining or P2P authority.
pub fn derive_dormant_native_gd08_order_from_ghostdag_trial(
    chain_id: &str,
    frozen_genesis_timestamp_ns: u64,
    frozen_genesis_hash: &str,
    tip: &BlockId,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
    trial_k: usize,
) -> Result<DormantGd08OrderPreview, PulseError> {
    let trial = derive_dormant_native_v3_ghostdag_trial(
        chain_id,
        frozen_genesis_timestamp_ns,
        frozen_genesis_hash,
        std::slice::from_ref(tip),
        headers,
        trial_k,
    )?;

    let mut reversed_spine = Vec::new();
    let mut seen = BTreeSet::new();
    let mut next = tip.clone();
    loop {
        if !seen.insert(next.clone()) || seen.len() > trial.blocks.len() {
            return Err(reject("cycle or unbounded derived selected-parent spine"));
        }
        let metadata = trial
            .blocks
            .get(&next)
            .ok_or_else(|| reject("derived selected-parent header missing"))?;
        reversed_spine.push(next.clone());
        match &metadata.selected_parent {
            Some(parent) => next = parent.clone(),
            None if next == frozen_genesis_hash => break,
            None => return Err(reject("derived spine does not end at frozen genesis")),
        }
    }
    reversed_spine.reverse();

    let mut assignments = BTreeMap::new();
    for (index, hash) in reversed_spine.iter().enumerate() {
        if assignments
            .insert(
                hash.clone(),
                DormantGd08AnchorSlot {
                    anchor: index,
                    role: DormantGd08Role::Selected,
                },
            )
            .is_some()
        {
            return Err(reject("duplicate derived selected-spine role"));
        }
    }
    for (index, hash) in reversed_spine.iter().enumerate().skip(1) {
        let metadata = trial
            .blocks
            .get(hash)
            .ok_or_else(|| reject("missing derived merge-set metadata"))?;
        for (role, side_blocks) in [
            (DormantGd08Role::MergeBlue, &metadata.merge_blues),
            (DormantGd08Role::MergeRed, &metadata.merge_reds),
        ] {
            for side_hash in side_blocks {
                if !trial.blocks.contains_key(side_hash) {
                    return Err(reject("derived merge-set references unknown side header"));
                }
                if assignments
                    .insert(
                        side_hash.clone(),
                        DormantGd08AnchorSlot {
                            anchor: index,
                            role,
                        },
                    )
                    .is_some()
                {
                    return Err(reject("duplicate or selected derived side role"));
                }
            }
        }
    }
    if assignments.len() != trial.blocks.len() {
        return Err(reject("incomplete derived GHOSTDAG side-role coverage"));
    }
    derive_dormant_native_gd08_order(
        chain_id,
        frozen_genesis_timestamp_ns,
        frozen_genesis_hash,
        &reversed_spine,
        headers,
        &assignments,
    )
}

#[cfg(test)]
mod derived_ghostdag_bridge_tests {
    use super::*;
    use crate::{
        genesis_v3::dormant_native_genesis_envelope_v3_ns,
        pow_v3::evaluate_v3_header_pow,
        protocol::BLOCK_HEADER_VERSION_V3,
        types::compute_merkle_root,
    };

    const CHAIN: &str = "pulsedag-dormant-gd08-ghostdag-derived-bridge";
    const GENESIS_NS: u64 = 1_800_000_012_000_000_000;
    const EASY_BITS: u32 = 0x207f_ffff;

    fn mined(
        mut parents: Vec<BlockId>,
        timestamp_ns: u64,
        height: u64,
        blue_score: u64,
        marker: &str,
    ) -> (BlockId, BlockHeaderV3) {
        parents.sort();
        let mut header = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents,
            timestamp_ns,
            difficulty: EASY_BITS,
            nonce: 0,
            merkle_root: compute_merkle_root(&[]),
            state_root: marker.repeat(64),
            blue_score,
            height,
        };
        for nonce in 0..4096_u64 {
            header.nonce = nonce;
            let proof = evaluate_v3_header_pow(&header, CHAIN).unwrap();
            if proof.pow.accepted {
                return (proof.block_hash, header);
            }
        }
        panic!("easy target nonce not found");
    }

    fn sibling_join(trial_k: usize) -> (BlockId, BlockId, BlockId, BTreeMap<BlockId, BlockHeaderV3>) {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let g = genesis.hash;
        let mut headers = BTreeMap::from([(g.clone(), genesis.header)]);
        let (a, ha) = mined(vec![g.clone()], GENESIS_NS + 500_000_000, 1, 1, "11");
        let (b, hb) = mined(vec![g.clone()], GENESIS_NS + 600_000_000, 1, 1, "22");
        headers.insert(a.clone(), ha);
        headers.insert(b.clone(), hb);
        let (tip, ht) = mined(
            vec![a, b],
            GENESIS_NS + 1_000_000_000,
            2,
            2 + trial_k as u64,
            "33",
        );
        headers.insert(tip.clone(), ht);
        (g, tip, headers[&tip].parents[0].clone(), headers)
    }

    #[test]
    fn derived_blue_red_roles_and_order_are_authenticated_and_deterministic() {
        for k in [0, 1] {
            let (genesis, tip, _, headers) = sibling_join(k);
            let first = derive_dormant_native_gd08_order_from_ghostdag_trial(
                CHAIN, GENESIS_NS, &genesis, &tip, &headers, k,
            )
            .unwrap();
            assert_eq!(first.ordered.len(), 4);
            assert_eq!(first.ordered.first(), Some(&genesis));
            assert_eq!(first.ordered.last(), Some(&tip));
            assert_eq!(first.blue_count, k);
            assert_eq!(first.red_count, 1 - k);
            let reversed = headers
                .iter()
                .rev()
                .map(|(id, header)| (id.clone(), header.clone()))
                .collect();
            let second = derive_dormant_native_gd08_order_from_ghostdag_trial(
                CHAIN, GENESIS_NS, &genesis, &tip, &reversed, k,
            )
            .unwrap();
            assert_eq!(first, second);
        }
    }

    #[test]
    fn derived_order_rejects_wrong_network_or_missing_ancestor() {
        let (genesis, tip, side, mut headers) = sibling_join(1);
        assert!(derive_dormant_native_gd08_order_from_ghostdag_trial(
            "wrong-chain", GENESIS_NS, &genesis, &tip, &headers, 1,
        )
        .is_err());
        headers.remove(&side);
        assert!(derive_dormant_native_gd08_order_from_ghostdag_trial(
            CHAIN, GENESIS_NS, &genesis, &tip, &headers, 1,
        )
        .is_err());
    }
}
