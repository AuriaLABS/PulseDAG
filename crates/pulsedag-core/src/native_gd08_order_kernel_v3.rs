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
    let tip = selected_spine.last().ok_or_else(|| reject("missing selected tip"))?;
    let verified = derive_dormant_native_v3_topological_evaluation(
        chain_id,
        frozen_genesis_timestamp_ns,
        frozen_genesis_hash,
        std::slice::from_ref(tip),
        headers,
    )?;
    let reachable: BTreeSet<BlockId> =
        verified.ordered_reachable_headers.iter().cloned().collect();
    let selected: BTreeSet<BlockId> = selected_spine.iter().cloned().collect();
    if selected.len() != selected_spine.len() {
        return Err(reject("duplicate selected-spine header"));
    }
    if reachable.len() != assignments.len()
        || reachable.iter().any(|hash| !assignments.contains_key(hash))
        || assignments.keys().any(|hash| !reachable.contains(hash))
    {
        return Err(reject("missing, extra or duplicate role/anchor assignments"));
    }
    for (index, hash) in selected_spine.iter().enumerate() {
        if !reachable.contains(hash) {
            return Err(reject("selected spine references unverified header"));
        }
        if assignments.get(hash) != Some(&DormantGd08AnchorSlot {
            anchor: index,
            role: DormantGd08Role::Selected,
        }) {
            return Err(reject("selected-spine slot/role mismatch"));
        }
        if index > 0 {
            let header = headers.get(hash).ok_or_else(|| reject("missing spine header"))?;
            if !header.parents.contains(&selected_spine[index - 1]) {
                return Err(reject("claimed selected parent is not a direct header parent"));
            }
        }
    }
    for (hash, slot) in assignments {
        if selected.contains(hash) {
            continue;
        }
        if slot.role == DormantGd08Role::Selected || slot.anchor == 0
            || slot.anchor >= selected_spine.len()
        {
            return Err(reject("side header claims invalid selected role or anchor"));
        }
        let first_reachable_anchor = selected_spine.iter().position(|spine_hash| {
            ancestor_or_self(hash, spine_hash, headers)
        });
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
                headers[*hash].parents.iter().all(|parent| emitted.contains(parent))
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
        blue_count: assignments.values().filter(|s| s.role == DormantGd08Role::MergeBlue).count(),
        red_count: assignments.values().filter(|s| s.role == DormantGd08Role::MergeRed).count(),
        ordered,
        digest: hex::encode(hasher.finalize()),
    })
}
