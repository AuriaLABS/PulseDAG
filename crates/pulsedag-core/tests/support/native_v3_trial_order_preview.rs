//! **TEST ONLY**: an explicit, UNAPPROVED native-v3 candidate block-order preview.
//!
//! A PoW-authenticated selected-parent spine is NOT the full DAG block order.
//! This bounded sidecar assigns every reachable side block exactly once to
//! the first selected-chain merge anchor, then emits a parent-before-child
//! topological sequence. At equal anchor, candidate blues precede reds when
//! ready; priorities are proposed candidate blue-work/score/height/hash.
//!
//! CRITICAL: This is NEITHER frozen GHOSTDAG consensus nor canonical economic
//! ordering. No transaction/fee/reward or UTXO state may use this test preview.
//! Production monetary replay still uses `derive_ordered_dag_v2`. This
//! diagnostic never connects to mining, P2P, database, RPC or contracts.

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet},
};

use pulsedag_core::{
    errors::PulseError,
    header_v3::BlockHeaderV3,
    native_ghostdag_policy_candidate_v3::{
        derive_dormant_native_v3_ghostdag_trial, CandidateGhostdagEvaluation,
    },
    native_topological_evaluation_v3::derive_dormant_native_v3_topological_evaluation,
    types::BlockId,
};
use sha2::{Digest, Sha256};

const PREVIEW_DOMAIN: &[u8] = b"PulseDAG:NONNORMATIVE:native-v3-trial-order:v1";

fn reject(why: &str) -> PulseError {
    PulseError::InvalidBlock(format!("TEST ONLY native-v3 trial ordering: {why}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnapprovedTrialOrder {
    pub selected_spine: Vec<BlockId>,
    pub ordered_blocks: Vec<BlockId>,
    pub side_blues: Vec<BlockId>,
    pub side_reds: Vec<BlockId>,
    pub digest: String,
    pub trial_k: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Blue = 0,
    Red = 1,
    Selected = 2,
}

/// Every block belongs to ONE selected-chain position, independent of
/// unordered map insertion. Nothing is removed from the authenticated DAG.
pub fn preview_unapproved_native_v3_order(
    chain_id: &str,
    genesis_ns: u64,
    genesis_hash: &str,
    tips: &[BlockId],
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
    trial_k: usize,
) -> Result<UnapprovedTrialOrder, PulseError> {
    let verified = derive_dormant_native_v3_topological_evaluation(
        chain_id, genesis_ns, genesis_hash, tips, headers,
    )?;
    let trial = derive_dormant_native_v3_ghostdag_trial(
        chain_id, genesis_ns, genesis_hash, tips, headers, trial_k,
    )?;
    // This preview does not even purport to choose among disconnected tips.
    if tips.len() != 1 {
        return Err(reject("one explicitly specified final tip is required"));
    }
    let spine = selected_spine(&trial, &tips[0], genesis_hash)?;
    let spine_ids: BTreeSet<BlockId> = spine.iter().cloned().collect();
    let mut priority: BTreeMap<BlockId, (usize, Kind)> = BTreeMap::new();
    for (i, hash) in spine.iter().enumerate() {
        if priority.insert(hash.clone(), (i, Kind::Selected)).is_some() {
            return Err(reject("duplicate selected-spine block"));
        }
    }
    for (i, anchor) in spine.iter().enumerate() {
        let meta = trial.blocks.get(anchor).ok_or_else(|| reject("missing anchor"))?;
        for (kind, members) in [
            (Kind::Blue, &meta.merge_blues),
            (Kind::Red, &meta.merge_reds),
        ] {
            for hash in members {
                if spine_ids.contains(hash) {
                    return Err(reject("merge side contains selected-chain block"));
                }
                if !trial.blocks.contains_key(hash) {
                    return Err(reject("unverified merge-side hash"));
                }
                if priority.insert(hash.clone(), (i, kind)).is_some() {
                    return Err(reject("side block classified twice"));
                }
            }
        }
    }
    let reachable = &verified.ordered_reachable_headers;
    if priority.len() != reachable.len()
        || reachable.iter().any(|hash| !priority.contains_key(hash))
    {
        return Err(reject("at least one authenticated side block is unclassified"));
    }
    if reachable.len() != trial.blocks.len() {
        return Err(reject("inconsistent complete authenticated ancestor closure"));
    }

    let mut remaining: BTreeSet<BlockId> = reachable.iter().cloned().collect();
    let mut emitted = BTreeSet::new();
    let mut ordered = Vec::with_capacity(remaining.len());
    while !remaining.is_empty() {
        let chosen = remaining.iter().filter(|hash| {
            headers[*hash].parents.iter().all(|parent| emitted.contains(parent))
        }).min_by_key(|hash| {
            let meta = &trial.blocks[*hash];
            let (slot, kind) = priority[*hash];
            let high_to_low_work: [u64; 8] = std::array::from_fn(|j| meta.blue_work.0[7 - j]);
            (
                slot,
                kind as u8,
                Reverse(high_to_low_work),
                Reverse(meta.blue_score),
                Reverse(headers[*hash].height),
                (*hash).clone(),
            )
        }).ok_or_else(|| reject("no topologically available block"))?.clone();
        remaining.remove(&chosen);
        emitted.insert(chosen.clone());
        ordered.push(chosen);
    }
    if ordered.first().map(String::as_str) != Some(genesis_hash) {
        return Err(reject("canonical trusted genesis not first"));
    }
    let side_blues = ordered.iter().filter(|hash| {
        priority[*hash].1 == Kind::Blue
    }).cloned().collect();
    let side_reds = ordered.iter().filter(|hash| {
        priority[*hash].1 == Kind::Red
    }).cloned().collect();
    let mut hasher = Sha256::new();
    hasher.update(PREVIEW_DOMAIN);
    hasher.update((trial_k as u64).to_le_bytes());
    hasher.update((ordered.len() as u64).to_le_bytes());
    for hash in &ordered {
        let bytes = hash.as_bytes();
        hasher.update((bytes.len() as u32).to_le_bytes());
        hasher.update(bytes);
        let kind = priority[hash].1 as u8;
        hasher.update([kind]);
    }
    Ok(UnapprovedTrialOrder {
        selected_spine: spine,
        ordered_blocks: ordered,
        side_blues,
        side_reds,
        digest: hex::encode(hasher.finalize()),
        trial_k,
    })
}

fn selected_spine(
    trial: &CandidateGhostdagEvaluation,
    tip: &str,
    genesis: &str,
) -> Result<Vec<BlockId>, PulseError> {
    let mut reversed = Vec::new();
    let mut seen = BTreeSet::new();
    let mut node = Some(tip.to_owned());
    while let Some(hash) = node {
        if !seen.insert(hash.clone()) || seen.len() > trial.blocks.len() {
            return Err(reject("cyclic selected-spine path"));
        }
        let meta = trial.blocks.get(&hash)
            .ok_or_else(|| reject("missing selected ancestor"))?;
        if meta.selected_parent.is_none() && hash != genesis {
            return Err(reject("selected chain ends outside frozen genesis"));
        }
        node = meta.selected_parent.clone();
        reversed.push(hash);
    }
    reversed.reverse();
    if reversed.first().map(String::as_str) != Some(genesis) {
        return Err(reject("selected spine is not rooted at trusted genesis"));
    }
    Ok(reversed)
}
