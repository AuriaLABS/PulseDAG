use std::{cmp::Ordering, collections::{BTreeMap, BTreeSet, VecDeque}};

use sha2::{Digest, Sha256};

use crate::{
    mempool::{canonical_mempool_txids, prune_expired_mempool},
    mempool_v3::canonical_transaction_size_for_mempool_v3,
    state::ChainState,
    types::{Hash, Transaction},
};

pub const MEMPOOL_RESOURCE_POLICY_V1_VERSION: u32 = 1;
const MEMPOOL_RESOURCE_POLICY_V1_FINGERPRINT_DOMAIN: &[u8] = b"PulseDAG:mempool-resource-policy:v1";
pub const MEMPOOL_RESOURCE_V1_MAX_TRANSACTIONS: u64 = 4_096;
pub const MEMPOOL_RESOURCE_V1_MAX_SPENT_OUTPOINTS: u64 = 8_192;
pub const MEMPOOL_RESOURCE_V1_MAX_ORPHANS: u64 = 512;
pub const MEMPOOL_RESOURCE_V1_MAX_CANONICAL_TX_BYTES: u64 = 32 * 1024;
pub const MEMPOOL_RESOURCE_V1_MAX_AGE_BLOCKS: u64 = 1_440;
pub const MEMPOOL_RESOURCE_TX_TOO_LARGE_CODE: &str = "MEMPOOL_RESOURCE_TX_TOO_LARGE";
const RESOURCE_REASON_SEPARATOR: &str = ": ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MempoolResourcePolicyV1 {
    pub version: u32,
    pub max_transactions: u64,
    pub max_spent_outpoints: u64,
    pub max_orphans: u64,
    pub max_canonical_tx_bytes: u64,
    pub max_age_blocks: u64,
}

impl MempoolResourcePolicyV1 {
    pub const fn production_default() -> Self {
        Self {
            version: MEMPOOL_RESOURCE_POLICY_V1_VERSION,
            max_transactions: MEMPOOL_RESOURCE_V1_MAX_TRANSACTIONS,
            max_spent_outpoints: MEMPOOL_RESOURCE_V1_MAX_SPENT_OUTPOINTS,
            max_orphans: MEMPOOL_RESOURCE_V1_MAX_ORPHANS,
            max_canonical_tx_bytes: MEMPOOL_RESOURCE_V1_MAX_CANONICAL_TX_BYTES,
            max_age_blocks: MEMPOOL_RESOURCE_V1_MAX_AGE_BLOCKS,
        }
    }

    pub fn fingerprint(self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(MEMPOOL_RESOURCE_POLICY_V1_FINGERPRINT_DOMAIN);
        hasher.update(self.version.to_le_bytes());
        hasher.update(self.max_transactions.to_le_bytes());
        hasher.update(self.max_spent_outpoints.to_le_bytes());
        hasher.update(self.max_orphans.to_le_bytes());
        hasher.update(self.max_canonical_tx_bytes.to_le_bytes());
        hasher.update(self.max_age_blocks.to_le_bytes());
        hex::encode(hasher.finalize())
    }
}

impl Default for MempoolResourcePolicyV1 {
    fn default() -> Self {
        Self::production_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MempoolResourceAssessmentErrorV1 {
    TransactionTooLarge {
        canonical_size_bytes: u64,
        max_canonical_tx_bytes: u64,
    },
    CanonicalSize(String),
}

pub fn assess_production_transaction_resources_v1(
    tx: &Transaction,
    state: &ChainState,
) -> Result<u64, MempoolResourceAssessmentErrorV1> {
    let policy = MempoolResourcePolicyV1::production_default();
    let canonical_size_bytes = canonical_transaction_size_for_mempool_v3(tx, &state.chain_id)
        .map_err(|error| MempoolResourceAssessmentErrorV1::CanonicalSize(format!("{error:?}")))?;
    if canonical_size_bytes > policy.max_canonical_tx_bytes {
        return Err(MempoolResourceAssessmentErrorV1::TransactionTooLarge {
            canonical_size_bytes,
            max_canonical_tx_bytes: policy.max_canonical_tx_bytes,
        });
    }
    Ok(canonical_size_bytes)
}

pub fn mempool_resource_rejection_reason_v1(
    canonical_size_bytes: u64,
    max_canonical_tx_bytes: u64,
) -> String {
    format!(
        "{MEMPOOL_RESOURCE_TX_TOO_LARGE_CODE}{RESOURCE_REASON_SEPARATOR}canonical transaction size {canonical_size_bytes} exceeds production maximum {max_canonical_tx_bytes} bytes"
    )
}

pub fn mempool_resource_rejection_code_from_reason_v1(reason: &str) -> Option<&str> {
    let code = reason
        .split_once(RESOURCE_REASON_SEPARATOR)
        .map(|(code, _)| code)
        .unwrap_or(reason);
    (code == MEMPOOL_RESOURCE_TX_TOO_LARGE_CODE).then_some(code)
}

pub fn mempool_resource_rejection_detail_v1(reason: &str) -> &str {
    reason
        .split_once(RESOURCE_REASON_SEPARATOR)
        .map(|(_, detail)| detail)
        .unwrap_or(reason)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MempoolResourceNormalizationV1 {
    pub removed_live_txids: Vec<Hash>,
    pub expired_live_txids: Vec<Hash>,
    pub dropped_orphan_txids: Vec<Hash>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PackageScoreV1 {
    total_fee: u128,
    tx_count: usize,
    max_member_fee: u64,
    canonical_txid: Hash,
}

fn score_cmp(a: &PackageScoreV1, b: &PackageScoreV1) -> Ordering {
    let a_weighted = a.total_fee.saturating_mul(b.tx_count as u128);
    let b_weighted = b.total_fee.saturating_mul(a.tx_count as u128);
    a_weighted
        .cmp(&b_weighted)
        .then_with(|| a.total_fee.cmp(&b.total_fee))
        .then_with(|| a.max_member_fee.cmp(&b.max_member_fee))
        .then_with(|| b.canonical_txid.cmp(&a.canonical_txid))
}

fn production_usize_ceiling(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

fn clamp_live_resource_limits(state: &mut ChainState, policy: MempoolResourcePolicyV1) {
    state.mempool.max_transactions = state
        .mempool
        .max_transactions
        .min(production_usize_ceiling(policy.max_transactions));
    state.mempool.max_spent_outpoints = state
        .mempool
        .max_spent_outpoints
        .min(production_usize_ceiling(policy.max_spent_outpoints));
    state.mempool.max_orphans = state
        .mempool
        .max_orphans
        .min(production_usize_ceiling(policy.max_orphans));
}

fn live_children(state: &ChainState) -> BTreeMap<Hash, BTreeSet<Hash>> {
    let mut children = BTreeMap::<Hash, BTreeSet<Hash>>::new();
    for tx in state.mempool.transactions.values() {
        for input in &tx.inputs {
            let parent = &input.previous_output.txid;
            if state.mempool.transactions.contains_key(parent) {
                children
                    .entry(parent.clone())
                    .or_default()
                    .insert(tx.txid.clone());
            }
        }
    }
    children
}

fn descendant_closure(
    roots: impl IntoIterator<Item = Hash>,
    children: &BTreeMap<Hash, BTreeSet<Hash>>,
) -> BTreeSet<Hash> {
    let mut removed = roots.into_iter().collect::<BTreeSet<_>>();
    let mut pending = removed.iter().cloned().collect::<VecDeque<_>>();
    while let Some(txid) = pending.pop_front() {
        if let Some(descendants) = children.get(&txid) {
            for descendant in descendants {
                if removed.insert(descendant.clone()) {
                    pending.push_back(descendant.clone());
                }
            }
        }
    }
    removed
}

fn rebuild_live_indexes(state: &mut ChainState) {
    let live = state
        .mempool
        .transactions
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    state.mempool.first_seen.retain(|txid, _| live.contains(txid));
    state
        .mempool
        .admission_height
        .retain(|txid, _| live.contains(txid));
    state.mempool.spent_outpoints.clear();
    for tx in state.mempool.transactions.values() {
        for input in &tx.inputs {
            state
                .mempool
                .spent_outpoints
                .insert(input.previous_output.clone());
        }
    }
}

fn remove_live_roots_package_safe(
    state: &mut ChainState,
    roots: impl IntoIterator<Item = Hash>,
) -> Vec<Hash> {
    let children = live_children(state);
    let removed = descendant_closure(roots, &children);
    for txid in &removed {
        state.mempool.transactions.remove(txid);
        state.mempool.first_seen.remove(txid);
        state.mempool.admission_height.remove(txid);
    }
    rebuild_live_indexes(state);
    removed.into_iter().collect()
}

fn package_score(package: &BTreeSet<Hash>, state: &ChainState) -> Option<PackageScoreV1> {
    let mut total_fee = 0_u128;
    let mut tx_count = 0usize;
    let mut max_member_fee = 0u64;
    let mut canonical_txid: Option<Hash> = None;
    for txid in package {
        let tx = state.mempool.transactions.get(txid)?;
        total_fee = total_fee.saturating_add(u128::from(tx.fee));
        tx_count = tx_count.saturating_add(1);
        max_member_fee = max_member_fee.max(tx.fee);
        canonical_txid = Some(match canonical_txid {
            Some(existing) => existing.min(tx.txid.clone()),
            None => tx.txid.clone(),
        });
    }
    Some(PackageScoreV1 {
        total_fee,
        tx_count,
        max_member_fee,
        canonical_txid: canonical_txid.unwrap_or_else(|| "-".to_string()),
    })
}

fn lowest_priority_live_package(state: &ChainState) -> Option<BTreeSet<Hash>> {
    let children = live_children(state);
    state
        .mempool
        .transactions
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|root| {
            let package = descendant_closure([root], &children);
            package_score(&package, state).map(|score| (package, score))
        })
        .min_by(|(_, a), (_, b)| score_cmp(a, b))
        .map(|(package, _)| package)
}

fn remove_orphan(state: &mut ChainState, txid: &str) -> bool {
    let removed = state.mempool.orphan_transactions.remove(txid).is_some();
    state.mempool.orphan_missing_outpoints.remove(txid);
    state.mempool.orphan_received_order.remove(txid);
    if removed {
        state.mempool.counters.orphan_dropped_total = state
            .mempool
            .counters
            .orphan_dropped_total
            .saturating_add(1);
        state.mempool.counters.orphan_pruned_total = state
            .mempool
            .counters
            .orphan_pruned_total
            .saturating_add(1);
    }
    removed
}

/// Enforce production resource ceilings on live/restored mempool state.
/// Existing stricter runtime/test caps remain stricter. Missing legacy
/// admission-height metadata is never synthesized and therefore retains
/// fail-safe under the #1081 expiry contract.
pub fn normalize_production_mempool_resources_v1(
    state: &mut ChainState,
) -> MempoolResourceNormalizationV1 {
    let policy = MempoolResourcePolicyV1::production_default();
    clamp_live_resource_limits(state, policy);

    let oversized_roots = state
        .mempool
        .transactions
        .iter()
        .filter_map(|(txid, tx)| {
            matches!(
                assess_production_transaction_resources_v1(tx, state),
                Err(MempoolResourceAssessmentErrorV1::TransactionTooLarge { .. })
            )
            .then_some(txid.clone())
        })
        .collect::<BTreeSet<_>>();
    let mut removed_live = remove_live_roots_package_safe(state, oversized_roots);

    let expiry = prune_expired_mempool(state, state.dag.best_height, policy.max_age_blocks);
    let expired_live_txids = expiry.expired_txids;
    removed_live.extend(expired_live_txids.iter().cloned());

    while state.mempool.transactions.len() > state.mempool.max_transactions
        || state.mempool.spent_outpoints.len() > state.mempool.max_spent_outpoints
    {
        let Some(package) = lowest_priority_live_package(state) else {
            state.mempool.spent_outpoints.clear();
            break;
        };
        removed_live.extend(remove_live_roots_package_safe(state, package));
    }

    let orphan_txids = state
        .mempool
        .orphan_transactions
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut dropped_orphans = BTreeSet::new();
    for txid in orphan_txids {
        let oversized = state
            .mempool
            .orphan_transactions
            .get(&txid)
            .is_some_and(|tx| {
                matches!(
                    assess_production_transaction_resources_v1(tx, state),
                    Err(MempoolResourceAssessmentErrorV1::TransactionTooLarge { .. })
                )
            });
        if oversized && remove_orphan(state, &txid) {
            dropped_orphans.insert(txid);
        }
    }

    if state.mempool.orphan_transactions.len() > state.mempool.max_orphans {
        let mut ordered = state
            .mempool
            .orphan_transactions
            .keys()
            .map(|txid| {
                (
                    state
                        .mempool
                        .orphan_received_order
                        .get(txid)
                        .copied()
                        .unwrap_or(u64::MAX),
                    txid.clone(),
                )
            })
            .collect::<Vec<_>>();
        ordered.sort();
        let overflow = state
            .mempool
            .orphan_transactions
            .len()
            .saturating_sub(state.mempool.max_orphans);
        for (_, txid) in ordered.into_iter().take(overflow) {
            if remove_orphan(state, &txid) {
                dropped_orphans.insert(txid);
            }
        }
    }

    let live_orphans = state
        .mempool
        .orphan_transactions
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    state
        .mempool
        .orphan_missing_outpoints
        .retain(|txid, _| live_orphans.contains(txid));
    state
        .mempool
        .orphan_received_order
        .retain(|txid, _| live_orphans.contains(txid));

    removed_live.sort();
    removed_live.dedup();
    MempoolResourceNormalizationV1 {
        removed_live_txids: removed_live,
        expired_live_txids,
        dropped_orphan_txids: dropped_orphans.into_iter().collect(),
    }
}

pub fn canonical_resource_survivors_v1(state: &ChainState) -> Vec<Hash> {
    canonical_mempool_txids(state)
}
