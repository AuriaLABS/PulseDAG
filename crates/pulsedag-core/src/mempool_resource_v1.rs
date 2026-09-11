use std::collections::{BTreeMap, BTreeSet, VecDeque};

use sha2::{Digest, Sha256};

use crate::{
    mempool::{mempool_logical_clock, prune_expired_mempool},
    mempool_v3::canonical_transaction_size_for_mempool_v3,
    state::ChainState,
    types::{Hash, Transaction},
};

pub const MEMPOOL_RESOURCE_POLICY_V1_VERSION: u32 = 1;
pub const MEMPOOL_RESOURCE_EXPIRY_BOUNDARY_V1: u32 = 1;
pub const MEMPOOL_RESOURCE_MAX_TRANSACTIONS_V1: u64 = 4_096;
pub const MEMPOOL_RESOURCE_MAX_SPENT_OUTPOINTS_V1: u64 = 8_192;
pub const MEMPOOL_RESOURCE_MAX_ORPHANS_V1: u64 = 512;
// Canonical transaction bytes and the serialized P2P transaction envelope are
// separate resource checks. The transport ceiling is deliberately not part of
// `MempoolResourcePolicyV1` so the frozen policy fingerprint remains unchanged.
pub const MEMPOOL_RESOURCE_MAX_TRANSACTION_BYTES_V1: u64 = 64 * 1_024;
pub const MEMPOOL_RESOURCE_MAX_TRANSACTION_MESSAGE_BYTES_V1: u64 = 64 * 1_024;
pub const MEMPOOL_RESOURCE_LIVE_MAX_AGE_BLOCKS_V1: u64 = 1_440;
pub const MEMPOOL_RESOURCE_ORPHAN_MAX_AGE_BLOCKS_V1: u64 = 1_440;
const MEMPOOL_RESOURCE_POLICY_V1_FINGERPRINT_DOMAIN: &[u8] = b"PulseDAG:mempool-resource-policy:v1";
const RESOURCE_REASON_SEPARATOR_V1: &str = ": ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MempoolResourcePolicyV1 {
    pub version: u32,
    pub max_transactions: u64,
    pub max_spent_outpoints: u64,
    pub max_orphans: u64,
    pub max_transaction_bytes: u64,
    pub live_max_age_blocks: u64,
    pub orphan_max_age_blocks: u64,
    pub expiry_boundary_version: u32,
}

impl Default for MempoolResourcePolicyV1 {
    fn default() -> Self {
        Self::production_default()
    }
}

impl MempoolResourcePolicyV1 {
    pub const fn production_default() -> Self {
        Self {
            version: MEMPOOL_RESOURCE_POLICY_V1_VERSION,
            max_transactions: MEMPOOL_RESOURCE_MAX_TRANSACTIONS_V1,
            max_spent_outpoints: MEMPOOL_RESOURCE_MAX_SPENT_OUTPOINTS_V1,
            max_orphans: MEMPOOL_RESOURCE_MAX_ORPHANS_V1,
            max_transaction_bytes: MEMPOOL_RESOURCE_MAX_TRANSACTION_BYTES_V1,
            live_max_age_blocks: MEMPOOL_RESOURCE_LIVE_MAX_AGE_BLOCKS_V1,
            orphan_max_age_blocks: MEMPOOL_RESOURCE_ORPHAN_MAX_AGE_BLOCKS_V1,
            expiry_boundary_version: MEMPOOL_RESOURCE_EXPIRY_BOUNDARY_V1,
        }
    }

    pub fn fingerprint(self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(MEMPOOL_RESOURCE_POLICY_V1_FINGERPRINT_DOMAIN);
        hasher.update(self.version.to_le_bytes());
        hasher.update(self.max_transactions.to_le_bytes());
        hasher.update(self.max_spent_outpoints.to_le_bytes());
        hasher.update(self.max_orphans.to_le_bytes());
        hasher.update(self.max_transaction_bytes.to_le_bytes());
        hasher.update(self.live_max_age_blocks.to_le_bytes());
        hasher.update(self.orphan_max_age_blocks.to_le_bytes());
        hasher.update(self.expiry_boundary_version.to_le_bytes());
        hex::encode(hasher.finalize())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MempoolResourceRejectionV1 {
    TransactionTooLarge,
}

impl MempoolResourceRejectionV1 {
    pub const fn as_code(self) -> &'static str {
        match self {
            Self::TransactionTooLarge => "MEMPOOL_RESOURCE_V1_TRANSACTION_TOO_LARGE",
        }
    }
}

pub fn mempool_resource_rejection_reason_v1(
    rejection: MempoolResourceRejectionV1,
    detail: impl AsRef<str>,
) -> String {
    format!(
        "{}{}{}",
        rejection.as_code(),
        RESOURCE_REASON_SEPARATOR_V1,
        detail.as_ref()
    )
}

pub fn mempool_resource_rejection_code_from_reason_v1(reason: &str) -> Option<&str> {
    let code = reason
        .split_once(RESOURCE_REASON_SEPARATOR_V1)
        .map(|(code, _)| code)
        .unwrap_or(reason);
    match code {
        "MEMPOOL_RESOURCE_V1_TRANSACTION_TOO_LARGE" => Some(code),
        _ => None,
    }
}

pub fn mempool_resource_rejection_detail_v1(reason: &str) -> &str {
    reason
        .split_once(RESOURCE_REASON_SEPARATOR_V1)
        .map(|(_, detail)| detail)
        .unwrap_or(reason)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MempoolResourceNormalizationV1 {
    pub seeded_live_admission_heights: u64,
    pub seeded_orphan_admission_heights: u64,
    pub expired_live_txids: Vec<Hash>,
    pub expired_orphan_txids: Vec<Hash>,
    pub removed_live_txids: Vec<Hash>,
    pub removed_orphan_txids: Vec<Hash>,
}

pub fn canonical_transaction_size_for_resource_v1(
    tx: &Transaction,
    chain_id: &str,
) -> Result<u64, String> {
    canonical_transaction_size_for_mempool_v3(tx, chain_id)
        .map_err(|error| format!("canonical resource-size assessment failed: {error:?}"))
}

#[derive(serde::Serialize)]
#[serde(tag = "type")]
enum TransactionCarrierMessageV1<'a> {
    NewTransaction {
        chain_id: &'a str,
        transaction: &'a Transaction,
    },
}

pub fn transaction_message_size_for_resource_v1(
    tx: &Transaction,
    chain_id: &str,
) -> Result<u64, String> {
    let bytes = serde_json::to_vec(&TransactionCarrierMessageV1::NewTransaction {
        chain_id,
        transaction: tx,
    })
    .map_err(|error| format!("transaction carrier serialization failed: {error}"))?;
    u64::try_from(bytes.len()).map_err(|_| "transaction carrier size overflow".to_string())
}

pub(crate) fn apply_production_mempool_resource_caps_v1(state: &mut ChainState) {
    let policy = MempoolResourcePolicyV1::production_default();
    state.mempool.max_transactions = policy.max_transactions as usize;
    state.mempool.max_spent_outpoints = policy.max_spent_outpoints as usize;
    state.mempool.max_orphans = policy.max_orphans as usize;
}

fn transaction_fits_production_resource_v1(
    tx: &Transaction,
    chain_id: &str,
    policy: MempoolResourcePolicyV1,
) -> bool {
    canonical_transaction_size_for_resource_v1(tx, chain_id)
        .is_ok_and(|size| size <= policy.max_transaction_bytes)
        && transaction_message_size_for_resource_v1(tx, chain_id)
            .is_ok_and(|size| size <= MEMPOOL_RESOURCE_MAX_TRANSACTION_MESSAGE_BYTES_V1)
}

fn live_children(state: &ChainState) -> BTreeMap<Hash, BTreeSet<Hash>> {
    let mut children = BTreeMap::<Hash, BTreeSet<Hash>>::new();
    for tx in state.mempool.transactions.values() {
        for input in &tx.inputs {
            if state
                .mempool
                .transactions
                .contains_key(&input.previous_output.txid)
            {
                children
                    .entry(input.previous_output.txid.clone())
                    .or_default()
                    .insert(tx.txid.clone());
            }
        }
    }
    children
}

fn package_from_roots(state: &ChainState, roots: impl IntoIterator<Item = Hash>) -> BTreeSet<Hash> {
    let children = live_children(state);
    let mut removal = BTreeSet::new();
    let mut pending = roots.into_iter().collect::<VecDeque<_>>();
    while let Some(txid) = pending.pop_front() {
        if !removal.insert(txid.clone()) {
            continue;
        }
        if let Some(kids) = children.get(&txid) {
            pending.extend(kids.iter().cloned());
        }
    }
    removal
}

fn rebuild_spent_outpoints(state: &mut ChainState) {
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

fn remove_live_package(state: &mut ChainState, roots: impl IntoIterator<Item = Hash>) -> Vec<Hash> {
    let package = package_from_roots(state, roots);
    let mut removed = Vec::new();
    for txid in package {
        if state.mempool.transactions.remove(&txid).is_some() {
            state.mempool.first_seen.remove(&txid);
            state.mempool.admission_height.remove(&txid);
            removed.push(txid);
        }
    }
    if !removed.is_empty() {
        rebuild_spent_outpoints(state);
    }
    removed
}

fn lowest_live_resource_victim(state: &ChainState) -> Option<Hash> {
    state
        .mempool
        .transactions
        .values()
        .min_by(|a, b| a.fee.cmp(&b.fee).then_with(|| b.txid.cmp(&a.txid)))
        .map(|tx| tx.txid.clone())
}

fn remove_orphan(state: &mut ChainState, txid: &str) -> bool {
    let removed = state.mempool.orphan_transactions.remove(txid).is_some();
    state.mempool.orphan_missing_outpoints.remove(txid);
    state.mempool.orphan_received_order.remove(txid);
    state.mempool.orphan_admission_height.remove(txid);
    if removed {
        state.mempool.counters.orphan_dropped_total = state
            .mempool
            .counters
            .orphan_dropped_total
            .saturating_add(1);
        state.mempool.counters.orphan_pruned_total =
            state.mempool.counters.orphan_pruned_total.saturating_add(1);
    }
    removed
}

fn logical_expired(current_height: u64, admission_height: u64, max_age_blocks: u64) -> bool {
    admission_height
        .checked_add(max_age_blocks)
        .map(|deadline| current_height >= deadline)
        .unwrap_or(false)
}

fn deterministic_orphan_victims(state: &ChainState) -> Vec<Hash> {
    let mut candidates = state
        .mempool
        .orphan_transactions
        .values()
        .map(|tx| (tx.fee, tx.txid.clone()))
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)));
    candidates.into_iter().map(|(_, txid)| txid).collect()
}

pub fn normalize_production_mempool_resources_v1(
    state: &mut ChainState,
) -> MempoolResourceNormalizationV1 {
    let policy = MempoolResourcePolicyV1::production_default();
    let current_height = mempool_logical_clock(state);
    let mut out = MempoolResourceNormalizationV1::default();

    apply_production_mempool_resource_caps_v1(state);

    let mut live_txids = state
        .mempool
        .transactions
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    live_txids.sort();
    for txid in live_txids {
        if let std::collections::hash_map::Entry::Vacant(entry) =
            state.mempool.admission_height.entry(txid)
        {
            entry.insert(current_height);
            out.seeded_live_admission_heights = out.seeded_live_admission_heights.saturating_add(1);
        }
    }
    let mut orphan_txids = state
        .mempool
        .orphan_transactions
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    orphan_txids.sort();
    for txid in orphan_txids {
        if let std::collections::hash_map::Entry::Vacant(entry) =
            state.mempool.orphan_admission_height.entry(txid)
        {
            entry.insert(current_height);
            out.seeded_orphan_admission_heights =
                out.seeded_orphan_admission_heights.saturating_add(1);
        }
    }

    let expiry = prune_expired_mempool(state, current_height, policy.live_max_age_blocks);
    out.expired_live_txids = expiry.expired_txids.clone();
    out.removed_live_txids.extend(expiry.expired_txids);

    let mut oversized_roots = Vec::new();
    let mut live = state
        .mempool
        .transactions
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    live.sort();
    for txid in live {
        let should_remove = state
            .mempool
            .transactions
            .get(&txid)
            .map(|tx| !transaction_fits_production_resource_v1(tx, &state.chain_id, policy))
            .unwrap_or(false);
        if should_remove {
            oversized_roots.push(txid);
        }
    }
    let removed_oversized = remove_live_package(state, oversized_roots);
    if !removed_oversized.is_empty() {
        state.mempool.counters.evicted_total = state
            .mempool
            .counters
            .evicted_total
            .saturating_add(removed_oversized.len() as u64);
        out.removed_live_txids.extend(removed_oversized);
    }

    rebuild_spent_outpoints(state);
    while state.mempool.transactions.len() > state.mempool.max_transactions
        || state.mempool.spent_outpoints.len() > state.mempool.max_spent_outpoints
    {
        let Some(victim) = lowest_live_resource_victim(state) else {
            break;
        };
        let removed = remove_live_package(state, std::iter::once(victim));
        if removed.is_empty() {
            break;
        }
        state.mempool.counters.evicted_total = state
            .mempool
            .counters
            .evicted_total
            .saturating_add(removed.len() as u64);
        out.removed_live_txids.extend(removed);
    }

    let mut expired_orphans = Vec::new();
    let mut invalid_or_oversized_orphans = Vec::new();
    let mut orphans = state
        .mempool
        .orphan_transactions
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    orphans.sort();
    for txid in orphans {
        let admission = state.mempool.orphan_admission_height.get(&txid).copied();
        if admission.is_some_and(|height| {
            logical_expired(current_height, height, policy.orphan_max_age_blocks)
        }) {
            expired_orphans.push(txid);
            continue;
        }
        let oversized = state
            .mempool
            .orphan_transactions
            .get(&txid)
            .map(|tx| !transaction_fits_production_resource_v1(tx, &state.chain_id, policy))
            .unwrap_or(false);
        if oversized {
            invalid_or_oversized_orphans.push(txid);
        }
    }
    for txid in &expired_orphans {
        if remove_orphan(state, txid) {
            out.expired_orphan_txids.push(txid.clone());
            out.removed_orphan_txids.push(txid.clone());
        }
    }
    for txid in invalid_or_oversized_orphans {
        if remove_orphan(state, &txid) {
            out.removed_orphan_txids.push(txid);
        }
    }

    let overflow = state
        .mempool
        .orphan_transactions
        .len()
        .saturating_sub(state.mempool.max_orphans);
    for txid in deterministic_orphan_victims(state)
        .into_iter()
        .take(overflow)
    {
        if remove_orphan(state, &txid) {
            out.removed_orphan_txids.push(txid);
        }
    }

    let live_set = state
        .mempool
        .transactions
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    state
        .mempool
        .first_seen
        .retain(|txid, _| live_set.contains(txid));
    state
        .mempool
        .admission_height
        .retain(|txid, _| live_set.contains(txid));
    let orphan_set = state
        .mempool
        .orphan_transactions
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    state
        .mempool
        .orphan_missing_outpoints
        .retain(|txid, _| orphan_set.contains(txid));
    state
        .mempool
        .orphan_received_order
        .retain(|txid, _| orphan_set.contains(txid));
    state
        .mempool
        .orphan_admission_height
        .retain(|txid, _| orphan_set.contains(txid));

    out.removed_live_txids.sort();
    out.removed_live_txids.dedup();
    out.removed_orphan_txids.sort();
    out.removed_orphan_txids.dedup();
    out.expired_live_txids.sort();
    out.expired_live_txids.dedup();
    out.expired_orphan_txids.sort();
    out.expired_orphan_txids.dedup();

    debug_assert!(production_mempool_resource_invariants_v1(state));
    out
}

pub fn production_mempool_resource_invariants_v1(state: &ChainState) -> bool {
    let policy = MempoolResourcePolicyV1::production_default();
    if state.mempool.max_transactions != policy.max_transactions as usize
        || state.mempool.max_spent_outpoints != policy.max_spent_outpoints as usize
        || state.mempool.max_orphans != policy.max_orphans as usize
        || state.mempool.transactions.len() > policy.max_transactions as usize
        || state.mempool.spent_outpoints.len() > policy.max_spent_outpoints as usize
        || state.mempool.orphan_transactions.len() > policy.max_orphans as usize
    {
        return false;
    }
    if state
        .mempool
        .transactions
        .values()
        .any(|tx| !transaction_fits_production_resource_v1(tx, &state.chain_id, policy))
    {
        return false;
    }
    let live = state.mempool.transactions.keys().collect::<BTreeSet<_>>();
    let orphans = state
        .mempool
        .orphan_transactions
        .keys()
        .collect::<BTreeSet<_>>();
    state
        .mempool
        .first_seen
        .keys()
        .all(|txid| live.contains(txid))
        && state
            .mempool
            .admission_height
            .keys()
            .all(|txid| live.contains(txid))
        && state
            .mempool
            .orphan_missing_outpoints
            .keys()
            .all(|txid| orphans.contains(txid))
        && state
            .mempool
            .orphan_received_order
            .keys()
            .all(|txid| orphans.contains(txid))
        && state
            .mempool
            .orphan_admission_height
            .keys()
            .all(|txid| orphans.contains(txid))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        types::{OutPoint, Transaction, TxInput, TxOutput},
        MempoolPolicyV3, TRANSACTION_VERSION_V1,
    };

    fn tx(txid: &str, fee: u64, parent: Option<&str>, address_len: usize) -> Transaction {
        Transaction {
            txid: txid.to_string(),
            version: TRANSACTION_VERSION_V1,
            inputs: parent
                .into_iter()
                .map(|parent| TxInput {
                    previous_output: OutPoint {
                        txid: parent.to_string(),
                        index: 0,
                    },
                    public_key: String::new(),
                    signature: String::new(),
                })
                .collect(),
            outputs: vec![TxOutput {
                address: "a".repeat(address_len),
                amount: 1,
            }],
            fee,
            nonce: 1,
        }
    }

    fn insert_live(state: &mut ChainState, transaction: Transaction, height: Option<u64>) {
        let txid = transaction.txid.clone();
        for input in &transaction.inputs {
            state
                .mempool
                .spent_outpoints
                .insert(input.previous_output.clone());
        }
        state.mempool.transactions.insert(txid.clone(), transaction);
        state.mempool.first_seen.insert(txid.clone(), 0);
        if let Some(height) = height {
            state.mempool.admission_height.insert(txid, height);
        }
    }

    fn insert_orphan(state: &mut ChainState, transaction: Transaction, height: Option<u64>) {
        let txid = transaction.txid.clone();
        state
            .mempool
            .orphan_transactions
            .insert(txid.clone(), transaction);
        state.mempool.orphan_received_order.insert(txid.clone(), 0);
        state
            .mempool
            .orphan_missing_outpoints
            .insert(txid.clone(), Vec::new());
        if let Some(height) = height {
            state.mempool.orphan_admission_height.insert(txid, height);
        }
    }

    #[test]
    fn production_resource_policy_golden_vector_is_frozen() {
        let policy = MempoolResourcePolicyV1::production_default();
        assert_eq!(policy.version, 1);
        assert_eq!(policy.max_transactions, 4_096);
        assert_eq!(policy.max_spent_outpoints, 8_192);
        assert_eq!(policy.max_orphans, 512);
        assert_eq!(policy.max_transaction_bytes, 65_536);
        assert_eq!(policy.live_max_age_blocks, 1_440);
        assert_eq!(policy.orphan_max_age_blocks, 1_440);
        assert_eq!(policy.expiry_boundary_version, 1);
        assert_eq!(
            policy.fingerprint(),
            "2fb3ff1fb784703fa197a362d1a42e9312a3b2ea6a1b3b9c9406ce1c2c588b3f"
        );
        assert_eq!(
            MempoolPolicyV3::production_default().fingerprint(),
            "fc08725ab79ace07323f11d085c2c105ed5f5e6338b67555103d8cb273c732c8"
        );
    }

    #[test]
    fn normalization_freezes_caps_and_seeds_missing_logical_ages() {
        let mut state = init_chain_state("resource-seed".into());
        state.dag.best_height = 77;
        state.mempool.max_transactions = 99;
        state.mempool.max_spent_outpoints = 100;
        state.mempool.max_orphans = 1;
        insert_live(&mut state, tx("live", 1, None, 8), None);
        insert_orphan(&mut state, tx("orphan", 1, None, 8), None);

        let result = normalize_production_mempool_resources_v1(&mut state);
        assert_eq!(result.seeded_live_admission_heights, 1);
        assert_eq!(result.seeded_orphan_admission_heights, 1);
        assert_eq!(state.mempool.admission_height.get("live"), Some(&77));
        assert_eq!(
            state.mempool.orphan_admission_height.get("orphan"),
            Some(&77)
        );
        assert!(production_mempool_resource_invariants_v1(&state));
    }

    #[test]
    fn exact_live_and_orphan_expiry_boundary_is_current_gte_admission_plus_ttl() {
        let mut state = init_chain_state("resource-expiry".into());
        state.dag.best_height = 1_439;
        insert_live(&mut state, tx("live", 1, None, 8), Some(0));
        insert_orphan(&mut state, tx("orphan", 1, None, 8), Some(0));
        let before = normalize_production_mempool_resources_v1(&mut state);
        assert!(before.expired_live_txids.is_empty());
        assert!(before.expired_orphan_txids.is_empty());

        state.dag.best_height = 1_440;
        let at_boundary = normalize_production_mempool_resources_v1(&mut state);
        assert_eq!(at_boundary.expired_live_txids, vec!["live".to_string()]);
        assert_eq!(at_boundary.expired_orphan_txids, vec!["orphan".to_string()]);
        assert!(state.mempool.transactions.is_empty());
        assert!(state.mempool.orphan_transactions.is_empty());
    }

    #[test]
    fn expiring_parent_removes_live_descendant_package_and_metadata() {
        let mut state = init_chain_state("resource-package-expiry".into());
        state.dag.best_height = 1_440;
        insert_live(&mut state, tx("parent", 1, None, 8), Some(0));
        insert_live(&mut state, tx("child", 2, Some("parent"), 8), Some(1_439));

        let result = normalize_production_mempool_resources_v1(&mut state);
        assert_eq!(
            result.expired_live_txids,
            vec!["child".to_string(), "parent".to_string()]
        );
        assert!(state.mempool.transactions.is_empty());
        assert!(state.mempool.spent_outpoints.is_empty());
        assert!(state.mempool.first_seen.is_empty());
        assert!(state.mempool.admission_height.is_empty());
    }

    #[test]
    fn canonical_oversize_removes_parent_and_descendant_package() {
        let mut state = init_chain_state("resource-size".into());
        let oversized = tx("oversized", 1, None, 70_000);
        let size = canonical_transaction_size_for_resource_v1(&oversized, &state.chain_id).unwrap();
        assert!(size > MEMPOOL_RESOURCE_MAX_TRANSACTION_BYTES_V1);
        insert_live(&mut state, oversized, Some(0));
        insert_live(&mut state, tx("child", 2, Some("oversized"), 8), Some(0));

        let result = normalize_production_mempool_resources_v1(&mut state);
        assert_eq!(
            result.removed_live_txids,
            vec!["child".to_string(), "oversized".to_string()]
        );
        assert!(state.mempool.transactions.is_empty());
        assert!(production_mempool_resource_invariants_v1(&state));
    }

    #[test]
    fn normalization_removes_canonical_fit_but_unrelayable_live_and_orphan() {
        let mut state = init_chain_state("resource-carrier-size".into());
        let mut live = tx("live-carrier-oversize", 1, None, 1);
        live.outputs[0].address = "\"".repeat(40_000);
        let mut orphan = live.clone();
        orphan.txid = "orphan-carrier-oversize".to_string();

        let canonical = canonical_transaction_size_for_resource_v1(&live, &state.chain_id).unwrap();
        let carrier = transaction_message_size_for_resource_v1(&live, &state.chain_id).unwrap();
        assert!(canonical <= MEMPOOL_RESOURCE_MAX_TRANSACTION_BYTES_V1);
        assert!(carrier > MEMPOOL_RESOURCE_MAX_TRANSACTION_MESSAGE_BYTES_V1);

        insert_live(&mut state, live, Some(0));
        insert_orphan(&mut state, orphan, Some(0));
        let result = normalize_production_mempool_resources_v1(&mut state);

        assert_eq!(
            result.removed_live_txids,
            vec!["live-carrier-oversize".to_string()]
        );
        assert_eq!(
            result.removed_orphan_txids,
            vec!["orphan-carrier-oversize".to_string()]
        );
        assert!(state.mempool.transactions.is_empty());
        assert!(state.mempool.orphan_transactions.is_empty());
    }

    #[test]
    fn restored_orphan_capacity_is_independent_of_insertion_order() {
        fn normalized(order: impl IntoIterator<Item = usize>) -> BTreeSet<String> {
            let mut state = init_chain_state("resource-orphan-order".into());
            for i in order {
                insert_orphan(
                    &mut state,
                    tx(&format!("orphan-{i:04}"), (i % 7) as u64, None, 8),
                    Some(0),
                );
            }
            normalize_production_mempool_resources_v1(&mut state);
            state.mempool.orphan_transactions.keys().cloned().collect()
        }
        let forward = normalized(0..520);
        let reverse = normalized((0..520).rev());
        assert_eq!(forward, reverse);
        assert_eq!(forward.len(), 512);
    }
}
