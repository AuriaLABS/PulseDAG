use crate::{
    apply::{apply_block, prepare_block_state},
    errors::PulseError,
    mempool::{
        combined_pressure_tier, mempool_pressure_bps, reconcile_mempool, MEMPOOL_PRESSURE_HIGH_BPS,
        MEMPOOL_PRESSURE_SATURATED_BPS,
    },
    pow_validation_result, selected_pow_name,
    state::ChainState,
    types::{Block, Transaction},
    validation::{missing_transaction_inputs, validate_block, validate_transaction},
};
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone, Copy)]
pub enum AcceptSource {
    Rpc,
    P2p,
    LocalMining,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", content = "reason", rename_all = "snake_case")]
pub enum TxAcceptanceResult {
    Accepted,
    Duplicate,
    Invalid(String),
    Orphan,
    Rejected(String),
}

impl TxAcceptanceResult {
    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted)
    }
}

fn classify_tx_validation_error(err: PulseError) -> TxAcceptanceResult {
    match err {
        PulseError::TxAlreadyExists => TxAcceptanceResult::Duplicate,
        PulseError::InvalidTransaction(msg) if msg.contains("mempool backpressure") => {
            TxAcceptanceResult::Rejected(msg)
        }
        PulseError::InvalidTransaction(msg) => TxAcceptanceResult::Invalid(msg),
        PulseError::InvalidTxid => TxAcceptanceResult::Invalid("invalid txid".into()),
        PulseError::InvalidSignature => TxAcceptanceResult::Invalid("invalid signature".into()),
        PulseError::DoubleSpend => TxAcceptanceResult::Invalid("double spend".into()),
        PulseError::InsufficientFunds => TxAcceptanceResult::Invalid("insufficient funds".into()),
        PulseError::UtxoNotFound => TxAcceptanceResult::Orphan,
        other => TxAcceptanceResult::Rejected(other.to_string()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", content = "reason", rename_all = "snake_case")]
pub enum BlockAcceptanceResult {
    Accepted,
    Duplicate,
    InvalidPow,
    MissingParent,
    InvalidTransaction,
    Malformed,
    Rejected(String),
}

impl BlockAcceptanceResult {
    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicBlockAcceptance {
    pub result: BlockAcceptanceResult,
    pub persisted: bool,
    pub committed: bool,
    pub broadcast: bool,
}

impl AtomicBlockAcceptance {
    pub fn rejected(result: BlockAcceptanceResult) -> Self {
        Self {
            result,
            persisted: false,
            committed: false,
            broadcast: false,
        }
    }
}

fn classify_block_validation_error(err: PulseError) -> BlockAcceptanceResult {
    match err {
        PulseError::BlockAlreadyExists => BlockAcceptanceResult::Duplicate,
        PulseError::InvalidBlock(msg) => {
            if msg.contains("consensus difficulty") || msg.contains("proof of work") {
                BlockAcceptanceResult::InvalidPow
            } else if msg.contains("missing parent") {
                BlockAcceptanceResult::MissingParent
            } else {
                BlockAcceptanceResult::Malformed
            }
        }
        PulseError::InvalidTransaction(_)
        | PulseError::InvalidTxid
        | PulseError::InvalidSignature
        | PulseError::DoubleSpend
        | PulseError::InsufficientFunds
        | PulseError::UtxoNotFound => BlockAcceptanceResult::InvalidTransaction,
        PulseError::InvalidStateRoot(_) => BlockAcceptanceResult::Rejected(err.to_string()),
        PulseError::MissingCoinbase
        | PulseError::MultipleCoinbase
        | PulseError::CoinbaseNotFirst
        | PulseError::ExcessiveCoinbaseReward
        | PulseError::DuplicateUtxoOutpoint(_)
        | PulseError::RewardOverflow => BlockAcceptanceResult::Malformed,
        other => BlockAcceptanceResult::Rejected(other.to_string()),
    }
}

fn is_higher_priority(candidate: &Transaction, existing: &Transaction) -> bool {
    candidate.fee > existing.fee
        || (candidate.fee == existing.fee && candidate.txid < existing.txid)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PackageScore {
    total_fee: u128,
    tx_count: usize,
    max_member_fee: u64,
    canonical_txid: String,
}

fn package_score_for_ids<'a, I>(txids: I, state: &'a ChainState) -> Option<PackageScore>
where
    I: IntoIterator<Item = &'a String>,
{
    let mut total_fee = 0_u128;
    let mut tx_count = 0usize;
    let mut max_member_fee = 0u64;
    let mut canonical: Option<&str> = None;
    for txid in txids {
        let tx = state.mempool.transactions.get(txid)?;
        total_fee = total_fee.saturating_add(tx.fee as u128);
        tx_count = tx_count.saturating_add(1);
        max_member_fee = max_member_fee.max(tx.fee);
        canonical = Some(match canonical {
            Some(existing) => {
                if tx.txid.as_str() < existing {
                    tx.txid.as_str()
                } else {
                    existing
                }
            }
            None => tx.txid.as_str(),
        });
    }
    let canonical_txid = canonical.unwrap_or("-").to_string();
    Some(PackageScore {
        total_fee,
        tx_count,
        max_member_fee,
        canonical_txid,
    })
}

fn score_cmp(a: &PackageScore, b: &PackageScore) -> Ordering {
    let a_weighted = a.total_fee.saturating_mul(b.tx_count as u128);
    let b_weighted = b.total_fee.saturating_mul(a.tx_count as u128);
    a_weighted
        .cmp(&b_weighted)
        .then_with(|| a.total_fee.cmp(&b.total_fee))
        .then_with(|| a.max_member_fee.cmp(&b.max_member_fee))
        .then_with(|| b.canonical_txid.cmp(&a.canonical_txid))
}

fn lowest_priority_txid(state: &ChainState) -> Option<String> {
    state
        .mempool
        .transactions
        .values()
        .min_by(|a, b| a.fee.cmp(&b.fee).then_with(|| b.txid.cmp(&a.txid)))
        .map(|tx| tx.txid.clone())
}

fn direct_mempool_parents(tx: &Transaction, state: &ChainState) -> Vec<String> {
    tx.inputs
        .iter()
        .filter_map(|input| {
            if state
                .mempool
                .transactions
                .contains_key(&input.previous_output.txid)
            {
                Some(input.previous_output.txid.clone())
            } else {
                None
            }
        })
        .collect()
}

fn collect_ancestor_set(tx: &Transaction, state: &ChainState) -> HashSet<String> {
    let mut ancestors = HashSet::new();
    let mut stack = direct_mempool_parents(tx, state);

    while let Some(txid) = stack.pop() {
        if !ancestors.insert(txid.clone()) {
            continue;
        }
        if let Some(parent) = state.mempool.transactions.get(&txid) {
            stack.extend(direct_mempool_parents(parent, state));
        }
    }

    ancestors
}

fn incoming_package_score(tx: &Transaction, state: &ChainState) -> PackageScore {
    let ancestors = collect_ancestor_set(tx, state);
    let ancestor_score = package_score_for_ids(ancestors.iter(), state).unwrap_or(PackageScore {
        total_fee: 0,
        tx_count: 0,
        max_member_fee: 0,
        canonical_txid: "-".to_string(),
    });
    let canonical = if ancestors.is_empty() {
        tx.txid.as_str()
    } else {
        ancestors
            .iter()
            .map(|s| s.as_str())
            .chain(std::iter::once(tx.txid.as_str()))
            .min()
            .unwrap_or(tx.txid.as_str())
    };
    PackageScore {
        total_fee: ancestor_score.total_fee.saturating_add(tx.fee as u128),
        tx_count: ancestor_score.tx_count.saturating_add(1),
        max_member_fee: ancestor_score.max_member_fee.max(tx.fee),
        canonical_txid: canonical.to_string(),
    }
}

fn build_mempool_children(state: &ChainState) -> HashMap<String, Vec<String>> {
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for tx in state.mempool.transactions.values() {
        for parent_txid in direct_mempool_parents(tx, state) {
            children
                .entry(parent_txid)
                .or_default()
                .push(tx.txid.clone());
        }
    }
    children
}

fn collect_eviction_package(
    root_txid: &str,
    children: &HashMap<String, Vec<String>>,
) -> HashSet<String> {
    let mut package = HashSet::new();
    let mut stack = vec![root_txid.to_string()];

    while let Some(txid) = stack.pop() {
        if !package.insert(txid.clone()) {
            continue;
        }
        if let Some(kids) = children.get(&txid) {
            stack.extend(kids.iter().cloned());
        }
    }

    package
}

fn select_package_threshold<'a>(
    package: &HashSet<String>,
    state: &'a ChainState,
) -> Option<&'a Transaction> {
    package
        .iter()
        .filter_map(|txid| state.mempool.transactions.get(txid))
        .max_by(|a, b| a.fee.cmp(&b.fee).then_with(|| b.txid.cmp(&a.txid)))
}

fn prune_orphans(state: &mut ChainState) {
    if state.mempool.orphan_transactions.len() <= state.mempool.max_orphans {
        return;
    }
    let mut by_age = state
        .mempool
        .orphan_received_order
        .iter()
        .map(|(txid, order)| (txid.clone(), *order))
        .collect::<Vec<_>>();
    by_age.sort_by_key(|(_, order)| *order);

    let overflow = state
        .mempool
        .orphan_transactions
        .len()
        .saturating_sub(state.mempool.max_orphans);
    for (txid, _) in by_age.into_iter().take(overflow) {
        let removed = state.mempool.orphan_transactions.remove(&txid).is_some();
        state.mempool.orphan_missing_outpoints.remove(&txid);
        state.mempool.orphan_received_order.remove(&txid);
        if removed {
            state.mempool.counters.orphan_dropped_total = state
                .mempool
                .counters
                .orphan_dropped_total
                .saturating_add(1);
            state.mempool.counters.orphan_pruned_total =
                state.mempool.counters.orphan_pruned_total.saturating_add(1);
        }
    }
}

fn store_orphan_transaction(tx: Transaction, state: &mut ChainState) {
    let txid = tx.txid.clone();
    let missing = missing_transaction_inputs(&tx, state);
    let replaced = state
        .mempool
        .orphan_transactions
        .insert(txid.clone(), tx)
        .is_some();
    state
        .mempool
        .orphan_missing_outpoints
        .insert(txid.clone(), missing);
    if !replaced {
        let order = state.mempool.next_orphan_order;
        state.mempool.next_orphan_order = state.mempool.next_orphan_order.saturating_add(1);
        state.mempool.orphan_received_order.insert(txid, order);
        state.mempool.counters.orphaned_total =
            state.mempool.counters.orphaned_total.saturating_add(1);
    }
    prune_orphans(state);
}

fn remove_orphan_transaction(txid: &str, state: &mut ChainState) {
    state.mempool.orphan_transactions.remove(txid);
    state.mempool.orphan_missing_outpoints.remove(txid);
    state.mempool.orphan_received_order.remove(txid);
}

fn promote_ready_orphans(state: &mut ChainState, source: AcceptSource) {
    loop {
        let mut ready = state
            .mempool
            .orphan_transactions
            .iter()
            .filter_map(|(txid, tx)| {
                if missing_transaction_inputs(tx, state).is_empty() {
                    Some(txid.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if ready.is_empty() {
            break;
        }
        ready.sort();

        let mut promoted_any = false;
        for txid in ready {
            let Some(tx) = state.mempool.orphan_transactions.get(&txid).cloned() else {
                continue;
            };
            remove_orphan_transaction(&txid, state);
            match accept_transaction(tx.clone(), state, source) {
                Ok(()) => {
                    state.mempool.counters.orphan_promoted_total = state
                        .mempool
                        .counters
                        .orphan_promoted_total
                        .saturating_add(1);
                    promoted_any = true;
                }
                Err(PulseError::UtxoNotFound) => {
                    // Became unresolved again due to competing promotion; park back as orphan.
                    store_orphan_transaction(tx, state);
                }
                Err(_) => {
                    state.mempool.counters.orphan_dropped_total = state
                        .mempool
                        .counters
                        .orphan_dropped_total
                        .saturating_add(1);
                }
            }
        }
        if !promoted_any {
            break;
        }
    }
}

fn mempool_needs_reconcile(state: &ChainState) -> bool {
    let mut expected_spent = std::collections::HashSet::new();
    for tx in state.mempool.transactions.values() {
        for input in &tx.inputs {
            if !expected_spent.insert(input.previous_output.clone()) {
                return true;
            }
        }
    }
    expected_spent != state.mempool.spent_outpoints
}

pub fn accept_transaction(
    tx: Transaction,
    state: &mut ChainState,
    source: AcceptSource,
) -> Result<(), PulseError> {
    match accept_transaction_with_result(tx, state, source) {
        TxAcceptanceResult::Accepted | TxAcceptanceResult::Orphan => Ok(()),
        TxAcceptanceResult::Duplicate => Err(PulseError::TxAlreadyExists),
        TxAcceptanceResult::Invalid(reason) if reason == "double spend" => {
            Err(PulseError::DoubleSpend)
        }
        TxAcceptanceResult::Invalid(reason) => Err(PulseError::InvalidTransaction(reason)),
        TxAcceptanceResult::Rejected(reason) => Err(PulseError::InvalidTransaction(reason)),
    }
}

pub fn accept_transaction_with_result(
    tx: Transaction,
    state: &mut ChainState,
    source: AcceptSource,
) -> TxAcceptanceResult {
    if mempool_needs_reconcile(state) {
        reconcile_mempool(state);
    }

    if state.mempool.transactions.contains_key(&tx.txid)
        || state.mempool.orphan_transactions.contains_key(&tx.txid)
    {
        state.mempool.counters.rejected_total =
            state.mempool.counters.rejected_total.saturating_add(1);
        return TxAcceptanceResult::Duplicate;
    }

    if let Err(err) = validate_transaction(&tx, state) {
        if matches!(err, PulseError::UtxoNotFound) {
            store_orphan_transaction(tx, state);
            return TxAcceptanceResult::Orphan;
        }
        state.mempool.counters.rejected_total =
            state.mempool.counters.rejected_total.saturating_add(1);
        return classify_tx_validation_error(err);
    }
    if state.mempool.transactions.len() >= state.mempool.max_transactions {
        state.mempool.counters.pressure_events_total = state
            .mempool
            .counters
            .pressure_events_total
            .saturating_add(1);

        let Some(lowest_txid) = lowest_priority_txid(state) else {
            return TxAcceptanceResult::Rejected(
                "mempool pressure detected with no eviction candidate".into(),
            );
        };
        let children = build_mempool_children(state);
        let protected_ancestors = collect_ancestor_set(&tx, state);
        let candidate_score = incoming_package_score(&tx, state);

        let mut eviction_candidates = state
            .mempool
            .transactions
            .values()
            .map(|candidate| candidate.txid.clone())
            .collect::<Vec<_>>();
        eviction_candidates.sort_by(|a, b| {
            let a_tx = state
                .mempool
                .transactions
                .get(a)
                .expect("sorted eviction candidate exists");
            let b_tx = state
                .mempool
                .transactions
                .get(b)
                .expect("sorted eviction candidate exists");
            a_tx.fee
                .cmp(&b_tx.fee)
                .then_with(|| b_tx.txid.cmp(&a_tx.txid))
        });
        if let Some(pos) = eviction_candidates
            .iter()
            .position(|txid| txid == &lowest_txid)
        {
            if pos != 0 {
                eviction_candidates.swap(0, pos);
            }
        }

        let mut selected_package: Option<HashSet<String>> = None;
        for candidate_txid in eviction_candidates {
            let package = collect_eviction_package(&candidate_txid, &children);
            if package
                .iter()
                .any(|member| protected_ancestors.contains(member))
            {
                continue;
            }
            let Some(package_threshold) = select_package_threshold(&package, state) else {
                continue;
            };
            let Some(package_score) = package_score_for_ids(package.iter(), state) else {
                continue;
            };
            let candidate_beats_package = score_cmp(&candidate_score, &package_score).is_gt()
                || (score_cmp(&candidate_score, &package_score).is_eq()
                    && is_higher_priority(&tx, package_threshold));
            if candidate_beats_package {
                selected_package = Some(package);
                break;
            }
        }

        let Some(selected_package) = selected_package else {
            let tx_pressure_bps = mempool_pressure_bps(
                state.mempool.transactions.len(),
                state.mempool.max_transactions,
            );
            let orphan_pressure_bps = mempool_pressure_bps(
                state.mempool.orphan_transactions.len(),
                state.mempool.max_orphans,
            );
            let pressure_tier = combined_pressure_tier(tx_pressure_bps, orphan_pressure_bps);
            state.mempool.counters.rejected_total =
                state.mempool.counters.rejected_total.saturating_add(1);
            state.mempool.counters.rejected_low_priority_total = state
                .mempool
                .counters
                .rejected_low_priority_total
                .saturating_add(1);
            return TxAcceptanceResult::Rejected(format!(
                "mempool backpressure active (tier={} tx_pressure_bps={} orphan_pressure_bps={} high_bps={} saturated_bps={}): transaction priority below threshold",
                pressure_tier.as_str(),
                tx_pressure_bps,
                orphan_pressure_bps,
                MEMPOOL_PRESSURE_HIGH_BPS,
                MEMPOOL_PRESSURE_SATURATED_BPS
            ));
        };

        let mut evicted_count = 0_u64;
        let sorted_package = selected_package.into_iter().collect::<BTreeSet<_>>();
        for package_txid in sorted_package {
            if let Some(evicted) = state.mempool.transactions.remove(&package_txid) {
                for input in &evicted.inputs {
                    state.mempool.spent_outpoints.remove(&input.previous_output);
                }
                evicted_count = evicted_count.saturating_add(1);
            }
        }
        state.mempool.counters.evicted_total = state
            .mempool
            .counters
            .evicted_total
            .saturating_add(evicted_count);
    }

    for input in &tx.inputs {
        state
            .mempool
            .spent_outpoints
            .insert(input.previous_output.clone());
    }
    state.mempool.transactions.insert(tx.txid.clone(), tx);
    state.mempool.counters.accepted_total = state.mempool.counters.accepted_total.saturating_add(1);
    promote_ready_orphans(state, source);
    TxAcceptanceResult::Accepted
}

pub fn accept_block_atomically<FPersist, FBroadcast>(
    block: Block,
    state: &mut ChainState,
    source: AcceptSource,
    persist: FPersist,
    broadcast: FBroadcast,
) -> Result<AtomicBlockAcceptance, PulseError>
where
    FPersist: FnOnce(&Block, &ChainState) -> Result<(), PulseError>,
    FBroadcast: FnOnce(&Block) -> Result<(), PulseError>,
{
    let enforce_pow = matches!(
        source,
        AcceptSource::Rpc | AcceptSource::P2p | AcceptSource::LocalMining
    );
    let pow = pow_validation_result(&block.header);
    if enforce_pow && !pow.accepted {
        return Ok(AtomicBlockAcceptance::rejected(
            BlockAcceptanceResult::InvalidPow,
        ));
    }

    let working = match prepare_block_state(&block, state) {
        Ok(working) => working,
        Err(err) => {
            return Ok(AtomicBlockAcceptance::rejected(
                classify_block_validation_error(err),
            ))
        }
    };

    persist(&block, &working)?;
    *state = working;
    broadcast(&block)?;
    Ok(AtomicBlockAcceptance {
        result: BlockAcceptanceResult::Accepted,
        persisted: true,
        committed: true,
        broadcast: true,
    })
}

pub fn accept_block_with_result(
    block: Block,
    state: &mut ChainState,
    source: AcceptSource,
) -> BlockAcceptanceResult {
    let enforce_pow = matches!(
        source,
        AcceptSource::Rpc | AcceptSource::P2p | AcceptSource::LocalMining
    );
    let pow = pow_validation_result(&block.header);
    if enforce_pow && !pow.accepted {
        return BlockAcceptanceResult::InvalidPow;
    }

    if let Err(err) = validate_block(&block, state) {
        return classify_block_validation_error(err);
    }

    if let Err(err) = apply_block(&block, state) {
        return classify_block_validation_error(err);
    }
    BlockAcceptanceResult::Accepted
}

pub fn accept_block(
    block: Block,
    state: &mut ChainState,
    source: AcceptSource,
) -> Result<(), PulseError> {
    match accept_block_with_result(block, state, source) {
        BlockAcceptanceResult::Accepted => Ok(()),
        BlockAcceptanceResult::Duplicate => Err(PulseError::BlockAlreadyExists),
        BlockAcceptanceResult::MissingParent => {
            Err(PulseError::InvalidBlock("missing parent".to_string()))
        }
        BlockAcceptanceResult::InvalidPow => Err(PulseError::InvalidBlock(format!(
            "pow rejected by current {} policy",
            selected_pow_name()
        ))),
        BlockAcceptanceResult::InvalidTransaction => Err(PulseError::InvalidBlock(
            "invalid transaction in block".to_string(),
        )),
        BlockAcceptanceResult::Malformed => {
            Err(PulseError::InvalidBlock("malformed block".to_string()))
        }
        BlockAcceptanceResult::Rejected(reason) => Err(PulseError::InvalidBlock(reason)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        mining::{
            build_candidate_block, build_coinbase_transaction, refresh_block_consensus_ids,
            refresh_block_consensus_ids_with_state,
        },
    };
    use std::collections::{BTreeMap, BTreeSet};

    type InvalidBlockCase = (
        &'static str,
        Box<dyn Fn(&ChainState) -> Block>,
        BlockAcceptanceResult,
    );

    #[derive(Debug, PartialEq, Eq)]
    struct DagMutationSnapshot {
        blocks: BTreeSet<String>,
        tips: BTreeSet<String>,
        children: BTreeMap<String, Vec<String>>,
        best_height: u64,
        utxos: BTreeSet<String>,
        address_index: BTreeMap<String, Vec<String>>,
        mempool_transactions: BTreeSet<String>,
        mempool_spent_outpoints: BTreeSet<String>,
        orphan_blocks: BTreeSet<String>,
        orphan_missing_parents: BTreeMap<String, Vec<String>>,
        orphan_received_at_ms: BTreeMap<String, u64>,
    }

    fn outpoint_key(outpoint: &crate::types::OutPoint) -> String {
        format!("{}:{}", outpoint.txid, outpoint.index)
    }

    fn snapshot_state(state: &ChainState) -> DagMutationSnapshot {
        let mut children = BTreeMap::new();
        for (parent, child_hashes) in &state.dag.children {
            let mut sorted = child_hashes.clone();
            sorted.sort();
            children.insert(parent.clone(), sorted);
        }

        let mut address_index = BTreeMap::new();
        for (address, outpoints) in &state.utxo.address_index {
            let mut sorted = outpoints.iter().map(outpoint_key).collect::<Vec<_>>();
            sorted.sort();
            address_index.insert(address.clone(), sorted);
        }

        let mut orphan_missing_parents = BTreeMap::new();
        for (block_hash, parent_hashes) in &state.orphan_missing_parents {
            let mut sorted = parent_hashes.clone();
            sorted.sort();
            orphan_missing_parents.insert(block_hash.clone(), sorted);
        }

        DagMutationSnapshot {
            blocks: state.dag.blocks.keys().cloned().collect(),
            tips: state.dag.tips.iter().cloned().collect(),
            children,
            best_height: state.dag.best_height,
            utxos: state.utxo.utxos.keys().map(outpoint_key).collect(),
            address_index,
            mempool_transactions: state.mempool.transactions.keys().cloned().collect(),
            mempool_spent_outpoints: state
                .mempool
                .spent_outpoints
                .iter()
                .map(outpoint_key)
                .collect(),
            orphan_blocks: state.orphan_blocks.keys().cloned().collect(),
            orphan_missing_parents,
            orphan_received_at_ms: state.orphan_received_at_ms.clone().into_iter().collect(),
        }
    }

    fn valid_acceptance_block(state: &ChainState, _hash: &str, coinbase_nonce: u64) -> Block {
        let parents = vec![state.dag.genesis_hash.clone()];
        let txs = vec![build_coinbase_transaction("miner1", 50, coinbase_nonce)];
        let mut block = build_candidate_block(parents, 1, 1, txs);
        refresh_block_consensus_ids_with_state(&mut block, state).unwrap();
        block
    }

    fn invalid_pow_acceptance_block(state: &ChainState) -> Block {
        let mut block = valid_acceptance_block(state, "taxonomy-invalid-pow", 11);
        block.header.difficulty = 0x0100_0000;
        block.header.nonce = 0;
        refresh_block_consensus_ids(&mut block);
        block
    }

    fn missing_parent_acceptance_block() -> Block {
        let txs = vec![build_coinbase_transaction("miner1", 50, 12)];
        build_candidate_block(vec!["missing-parent".into()], 1, 1, txs)
    }

    fn invalid_transaction_acceptance_block(state: &ChainState) -> Block {
        let coinbase = build_coinbase_transaction("miner1", 50, 13);
        let mut invalid_spend = crate::types::Transaction {
            txid: String::new(),
            version: 1,
            inputs: vec![crate::types::TxInput {
                previous_output: crate::types::OutPoint {
                    txid: "missing-utxo".to_string(),
                    index: 0,
                },
                public_key: "not-a-valid-public-key".to_string(),
                signature: "not-a-valid-signature".to_string(),
            }],
            outputs: vec![crate::types::TxOutput {
                address: "receiver".to_string(),
                amount: 1,
            }],
            fee: 0,
            nonce: 13,
        };
        invalid_spend.txid = crate::tx::compute_txid(&invalid_spend);
        let parents = vec![state.dag.genesis_hash.clone()];
        build_candidate_block(parents, 1, 1, vec![coinbase, invalid_spend])
    }

    fn malformed_acceptance_block(state: &ChainState) -> Block {
        let mut block = valid_acceptance_block(state, "taxonomy-malformed", 14);
        block.header.parents.clear();
        block
    }

    #[test]
    fn block_acceptance_result_taxonomy_names_are_stable() {
        let cases = [
            (BlockAcceptanceResult::Accepted, r#"{"status":"accepted"}"#),
            (
                BlockAcceptanceResult::Duplicate,
                r#"{"status":"duplicate"}"#,
            ),
            (
                BlockAcceptanceResult::InvalidPow,
                r#"{"status":"invalid_pow"}"#,
            ),
            (
                BlockAcceptanceResult::MissingParent,
                r#"{"status":"missing_parent"}"#,
            ),
            (
                BlockAcceptanceResult::InvalidTransaction,
                r#"{"status":"invalid_transaction"}"#,
            ),
            (
                BlockAcceptanceResult::Malformed,
                r#"{"status":"malformed"}"#,
            ),
            (
                BlockAcceptanceResult::Rejected("internal error: database unavailable".into()),
                r#"{"status":"rejected","reason":"internal error: database unavailable"}"#,
            ),
        ];

        for (result, expected_json) in cases {
            assert_eq!(serde_json::to_string(&result).unwrap(), expected_json);
        }
    }

    #[test]
    fn block_acceptance_valid_block_returns_accepted() {
        let mut state = init_chain_state("test".to_string());
        let block = valid_acceptance_block(&state, "taxonomy-accepted", 21);

        let outcome = accept_block_with_result(block, &mut state, AcceptSource::P2p);

        assert_eq!(outcome, BlockAcceptanceResult::Accepted);
    }

    #[test]
    fn block_acceptance_duplicate_block_returns_duplicate_and_does_not_mutate_state() {
        let mut state = init_chain_state("test".to_string());
        let block = valid_acceptance_block(&state, "taxonomy-duplicate", 22);
        assert_eq!(
            accept_block_with_result(block.clone(), &mut state, AcceptSource::P2p),
            BlockAcceptanceResult::Accepted
        );
        let before = snapshot_state(&state);

        let outcome = accept_block_with_result(block, &mut state, AcceptSource::P2p);
        let after = snapshot_state(&state);

        assert_eq!(outcome, BlockAcceptanceResult::Duplicate);
        assert_eq!(after, before, "duplicate block must not mutate DAG state");
    }

    #[test]
    fn block_acceptance_invalid_pow_returns_invalid_pow() {
        let mut state = init_chain_state("test".to_string());
        let block = invalid_pow_acceptance_block(&state);

        let outcome = accept_block_with_result(block, &mut state, AcceptSource::P2p);

        assert_eq!(outcome, BlockAcceptanceResult::InvalidPow);
    }

    #[test]
    fn block_acceptance_missing_parent_returns_missing_parent() {
        let mut state = init_chain_state("test".to_string());
        let block = missing_parent_acceptance_block();

        let outcome = accept_block_with_result(block, &mut state, AcceptSource::P2p);

        assert_eq!(outcome, BlockAcceptanceResult::MissingParent);
    }

    #[test]
    fn block_acceptance_invalid_transaction_returns_invalid_transaction() {
        let mut state = init_chain_state("test".to_string());
        let block = invalid_transaction_acceptance_block(&state);

        let outcome = accept_block_with_result(block, &mut state, AcceptSource::P2p);

        assert_eq!(outcome, BlockAcceptanceResult::InvalidTransaction);
    }

    #[test]
    fn block_acceptance_malformed_block_returns_malformed() {
        let mut state = init_chain_state("test".to_string());
        let block = malformed_acceptance_block(&state);

        let outcome = accept_block_with_result(block, &mut state, AcceptSource::P2p);

        assert_eq!(outcome, BlockAcceptanceResult::Malformed);
    }

    #[test]
    fn block_acceptance_unexpected_internal_rejection_returns_rejected_with_reason() {
        let outcome = classify_block_validation_error(PulseError::Internal(
            "database unavailable".to_string(),
        ));

        assert_eq!(
            outcome,
            BlockAcceptanceResult::Rejected("internal error: database unavailable".to_string())
        );
    }

    #[test]
    fn block_acceptance_source_does_not_change_validation_semantics() {
        let sources = [
            AcceptSource::Rpc,
            AcceptSource::P2p,
            AcceptSource::LocalMining,
        ];

        for source in sources {
            let mut state = init_chain_state(format!("valid-{source:?}"));
            let block = valid_acceptance_block(&state, &format!("taxonomy-valid-{source:?}"), 30);
            assert_eq!(
                accept_block_with_result(block, &mut state, source),
                BlockAcceptanceResult::Accepted,
                "valid block outcome changed for {source:?}"
            );

            let mut state = init_chain_state(format!("duplicate-{source:?}"));
            let block =
                valid_acceptance_block(&state, &format!("taxonomy-duplicate-{source:?}"), 31);
            assert_eq!(
                accept_block_with_result(block.clone(), &mut state, source),
                BlockAcceptanceResult::Accepted
            );
            assert_eq!(
                accept_block_with_result(block, &mut state, source),
                BlockAcceptanceResult::Duplicate,
                "duplicate outcome changed for {source:?}"
            );

            let mut state = init_chain_state(format!("invalid-pow-{source:?}"));
            let block = invalid_pow_acceptance_block(&state);
            assert_eq!(
                accept_block_with_result(block, &mut state, source),
                BlockAcceptanceResult::InvalidPow,
                "invalid PoW outcome changed for {source:?}"
            );

            let mut state = init_chain_state(format!("missing-parent-{source:?}"));
            assert_eq!(
                accept_block_with_result(missing_parent_acceptance_block(), &mut state, source),
                BlockAcceptanceResult::MissingParent,
                "missing parent outcome changed for {source:?}"
            );

            let mut state = init_chain_state(format!("invalid-tx-{source:?}"));
            let block = invalid_transaction_acceptance_block(&state);
            assert_eq!(
                accept_block_with_result(block, &mut state, source),
                BlockAcceptanceResult::InvalidTransaction,
                "invalid transaction outcome changed for {source:?}"
            );

            let mut state = init_chain_state(format!("malformed-{source:?}"));
            let block = malformed_acceptance_block(&state);
            assert_eq!(
                accept_block_with_result(block, &mut state, source),
                BlockAcceptanceResult::Malformed,
                "malformed outcome changed for {source:?}"
            );
        }
    }

    #[test]
    fn block_acceptance_invalid_blocks_do_not_mutate_dag_state() {
        let cases: Vec<InvalidBlockCase> = vec![
            (
                "invalid pow",
                Box::new(invalid_pow_acceptance_block),
                BlockAcceptanceResult::InvalidPow,
            ),
            (
                "missing parent",
                Box::new(|_| missing_parent_acceptance_block()),
                BlockAcceptanceResult::MissingParent,
            ),
            (
                "invalid transaction",
                Box::new(invalid_transaction_acceptance_block),
                BlockAcceptanceResult::InvalidTransaction,
            ),
            (
                "malformed",
                Box::new(malformed_acceptance_block),
                BlockAcceptanceResult::Malformed,
            ),
        ];

        for (case_name, build_block, expected) in cases {
            let mut state = init_chain_state(format!("mutation-{case_name}"));
            let before = snapshot_state(&state);
            let outcome =
                accept_block_with_result(build_block(&state), &mut state, AcceptSource::P2p);
            let after = snapshot_state(&state);

            assert_eq!(outcome, expected, "unexpected outcome for {case_name}");
            assert_eq!(after, before, "{case_name} must not mutate DAG state");
        }
    }

    #[test]
    fn rejects_block_with_invalid_pow() {
        let mut state = init_chain_state("test".to_string());
        let parents = vec![state.dag.genesis_hash.clone()];
        let txs = vec![build_coinbase_transaction("miner1", 50, 1)];
        let mut block = build_candidate_block(parents, 1, 0x01000000, txs);
        block.header.nonce = 0;

        let outcome = accept_block_with_result(block, &mut state, AcceptSource::P2p);
        assert_eq!(outcome, BlockAcceptanceResult::InvalidPow);
    }

    #[test]
    fn accepts_block_with_valid_pow() {
        let mut state = init_chain_state("test".to_string());
        let parents = vec![state.dag.genesis_hash.clone()];
        let txs = vec![build_coinbase_transaction("miner1", 50, 1)];
        let mut block = build_candidate_block(parents, 1, 1, txs);
        refresh_block_consensus_ids_with_state(&mut block, &state).unwrap();

        assert!(accept_block(block, &mut state, AcceptSource::P2p).is_ok());
    }

    #[test]
    fn duplicate_block_returns_duplicate_outcome() {
        let mut state = init_chain_state("test".to_string());
        let parents = vec![state.dag.genesis_hash.clone()];
        let txs = vec![build_coinbase_transaction("miner1", 50, 1)];
        let mut block = build_candidate_block(parents, 1, 1, txs);
        refresh_block_consensus_ids_with_state(&mut block, &state).unwrap();
        assert!(accept_block(block.clone(), &mut state, AcceptSource::P2p).is_ok());
        let outcome = accept_block_with_result(block, &mut state, AcceptSource::P2p);
        assert_eq!(outcome, BlockAcceptanceResult::Duplicate);
    }

    #[test]
    fn unknown_parent_returns_unknown_parent_outcome() {
        let mut state = init_chain_state("test".to_string());
        let txs = vec![build_coinbase_transaction("miner1", 50, 1)];
        let block = build_candidate_block(vec!["missing-parent".into()], 1, 1, txs);
        let outcome = accept_block_with_result(block, &mut state, AcceptSource::P2p);
        assert_eq!(outcome, BlockAcceptanceResult::MissingParent);
    }

    #[test]
    fn invalid_transaction_in_peer_block_returns_invalid_transaction_outcome() {
        let mut state = init_chain_state("test".to_string());
        let parents = vec![state.dag.genesis_hash.clone()];
        let coinbase = build_coinbase_transaction("miner1", 50, 1);
        let mut invalid_spend = crate::types::Transaction {
            txid: String::new(),
            version: 1,
            inputs: vec![crate::types::TxInput {
                previous_output: crate::types::OutPoint {
                    txid: "missing-utxo".to_string(),
                    index: 0,
                },
                public_key: "not-a-valid-public-key".to_string(),
                signature: "not-a-valid-signature".to_string(),
            }],
            outputs: vec![crate::types::TxOutput {
                address: "receiver".to_string(),
                amount: 1,
            }],
            fee: 0,
            nonce: 1,
        };
        invalid_spend.txid = crate::tx::compute_txid(&invalid_spend);
        let block = build_candidate_block(parents, 1, 1, vec![coinbase, invalid_spend]);

        let outcome = accept_block_with_result(block, &mut state, AcceptSource::P2p);
        assert_eq!(outcome, BlockAcceptanceResult::InvalidTransaction);
    }

    #[test]
    fn peer_block_and_mining_submit_share_canonical_acceptance_outcomes() {
        let parents = vec![init_chain_state("agreement".to_string()).dag.genesis_hash];
        let txs = vec![build_coinbase_transaction("miner1", 50, 1)];
        let mut peer_state = init_chain_state("agreement".to_string());
        let mut mining_state = init_chain_state("agreement".to_string());
        let mut block = build_candidate_block(parents, 1, 1, txs);
        refresh_block_consensus_ids_with_state(&mut block, &peer_state).unwrap();

        let peer_outcome =
            accept_block_with_result(block.clone(), &mut peer_state, AcceptSource::P2p);
        let mining_outcome =
            accept_block_with_result(block, &mut mining_state, AcceptSource::LocalMining);

        assert_eq!(peer_outcome, BlockAcceptanceResult::Accepted);
        assert_eq!(peer_outcome, mining_outcome);
        assert!(peer_state
            .dag
            .blocks
            .contains_key(&peer_state.dag.tips.iter().next().unwrap().clone()));
        assert!(mining_state
            .dag
            .blocks
            .contains_key(&mining_state.dag.tips.iter().next().unwrap().clone()));
    }

    #[test]
    fn acceptance_result_is_machine_readable() {
        let encoded = serde_json::to_string(&BlockAcceptanceResult::InvalidTransaction).unwrap();
        assert_eq!(encoded, "{\"status\":\"invalid_transaction\"}");
    }

    #[test]
    fn mutated_block_returns_invalid_structure() {
        let mut state = init_chain_state("test".to_string());
        let parents = vec![state.dag.genesis_hash.clone()];
        let mut txs = vec![build_coinbase_transaction("miner1", 50, 1)];
        let mut spend = txs[0].clone();
        spend.txid = "mutated".to_string();
        txs.push(spend);
        let mut block = build_candidate_block(parents, 1, 1, txs);
        block.hash = "mutated-block".to_string();
        let outcome = accept_block_with_result(block, &mut state, AcceptSource::P2p);
        assert_eq!(outcome, BlockAcceptanceResult::Malformed);
    }

    #[test]
    fn atomic_block_acceptance_storage_failure_leaves_live_state_unchanged_and_unbroadcast() {
        let mut state = init_chain_state("testnet".into());
        let before = snapshot_state(&state);
        let block = valid_acceptance_block(&state, "atomic-storage-fail", 77);
        let mut persisted = false;
        let mut broadcast = false;

        let err = accept_block_atomically(
            block,
            &mut state,
            AcceptSource::P2p,
            |_block, _working| {
                persisted = true;
                Err(PulseError::StorageError("forced storage failure".into()))
            },
            |_block| {
                broadcast = true;
                Ok(())
            },
        )
        .expect_err("storage failure bubbles out");

        assert!(matches!(err, PulseError::StorageError(_)));
        assert!(persisted, "acceptance reached durable persistence phase");
        assert!(!broadcast, "broadcast must not run before durable commit");
        assert_eq!(snapshot_state(&state), before);
    }

    #[test]
    fn atomic_block_acceptance_persists_before_commit_and_broadcast() {
        let mut state = init_chain_state("testnet".into());
        let block = valid_acceptance_block(&state, "atomic-success", 78);
        let block_hash = block.hash.clone();
        let order = std::cell::RefCell::new(Vec::new());

        let acceptance = accept_block_atomically(
            block,
            &mut state,
            AcceptSource::P2p,
            |block, working| {
                assert!(working.dag.blocks.contains_key(&block.hash));
                order.borrow_mut().push("persist");
                Ok(())
            },
            |_block| {
                order.borrow_mut().push("broadcast");
                Ok(())
            },
        )
        .expect("atomic acceptance succeeds");

        assert!(acceptance.result.is_accepted());
        assert!(acceptance.persisted);
        assert!(acceptance.committed);
        assert!(acceptance.broadcast);
        assert_eq!(*order.borrow(), vec!["persist", "broadcast"]);
        assert!(state.dag.blocks.contains_key(&block_hash));
    }
}

#[cfg(test)]
mod tx_acceptance_result_tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        tx::{address_from_public_key, compute_txid, signing_message},
        types::{OutPoint, Transaction, TxInput, TxOutput, Utxo},
    };
    use ed25519_dalek::{Signer, SigningKey};

    fn signing_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn public_key_hex(signing_key: &SigningKey) -> String {
        hex::encode(signing_key.verifying_key().to_bytes())
    }

    fn fund_address(state: &mut ChainState, txid: &str, address: String, amount: u64) -> OutPoint {
        let outpoint = OutPoint {
            txid: txid.to_string(),
            index: 0,
        };
        state.utxo.utxos.insert(
            outpoint.clone(),
            Utxo {
                outpoint: outpoint.clone(),
                address: address.clone(),
                amount,
                coinbase: false,
                height: 1,
            },
        );
        state
            .utxo
            .address_index
            .entry(address)
            .or_default()
            .push(outpoint.clone());
        outpoint
    }

    fn signed_tx(signing_key: &SigningKey, previous_output: OutPoint, nonce: u64) -> Transaction {
        let public_key = public_key_hex(signing_key);
        let mut tx = Transaction {
            txid: String::new(),
            version: 1,
            inputs: vec![TxInput {
                previous_output,
                public_key: public_key.clone(),
                signature: String::new(),
            }],
            outputs: vec![TxOutput {
                address: address_from_public_key(&public_key),
                amount: 9,
            }],
            fee: 1,
            nonce,
        };
        let signature = signing_key.sign(&signing_message(&tx));
        tx.inputs[0].signature = hex::encode(signature.to_bytes());
        tx.txid = compute_txid(&tx);
        tx
    }

    #[test]
    fn valid_p2p_tx_reaches_mempool_and_duplicate_is_classified() {
        let mut state = init_chain_state("testnet".into());
        let key = signing_key(42);
        let outpoint = fund_address(
            &mut state,
            "funding",
            address_from_public_key(&public_key_hex(&key)),
            10,
        );
        let tx = signed_tx(&key, outpoint, 1);
        let txid = tx.txid.clone();

        assert_eq!(
            accept_transaction_with_result(tx.clone(), &mut state, AcceptSource::P2p),
            TxAcceptanceResult::Accepted
        );
        assert!(state.mempool.transactions.contains_key(&txid));
        assert_eq!(
            accept_transaction_with_result(tx, &mut state, AcceptSource::P2p),
            TxAcceptanceResult::Duplicate
        );
    }

    #[test]
    fn invalid_and_orphan_p2p_txs_are_classified_without_mempool_acceptance() {
        let mut state = init_chain_state("testnet".into());
        let key = signing_key(7);
        let missing_outpoint = OutPoint {
            txid: "missing".into(),
            index: 0,
        };
        let orphan = signed_tx(&key, missing_outpoint, 1);
        let orphan_txid = orphan.txid.clone();
        assert_eq!(
            accept_transaction_with_result(orphan, &mut state, AcceptSource::P2p),
            TxAcceptanceResult::Orphan
        );
        assert!(state.mempool.orphan_transactions.contains_key(&orphan_txid));

        let invalid = Transaction {
            txid: "invalid".into(),
            version: 1,
            inputs: vec![],
            outputs: vec![],
            fee: 0,
            nonce: 0,
        };
        assert!(matches!(
            accept_transaction_with_result(invalid, &mut state, AcceptSource::P2p),
            TxAcceptanceResult::Invalid(_)
        ));
    }
}
