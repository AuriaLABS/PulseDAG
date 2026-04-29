use crate::{
    apply::apply_block,
    errors::PulseError,
    mempool::{
        combined_pressure_tier, mempool_pressure_bps, reconcile_mempool, MEMPOOL_PRESSURE_HIGH_BPS,
        MEMPOOL_PRESSURE_SATURATED_BPS,
    },
    pow_evaluate, selected_pow_name,
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
    if mempool_needs_reconcile(state) {
        reconcile_mempool(state);
    }

    if state.mempool.transactions.contains_key(&tx.txid)
        || state.mempool.orphan_transactions.contains_key(&tx.txid)
    {
        state.mempool.counters.rejected_total =
            state.mempool.counters.rejected_total.saturating_add(1);
        return Err(PulseError::TxAlreadyExists);
    }

    if let Err(err) = validate_transaction(&tx, state) {
        if matches!(err, PulseError::UtxoNotFound) {
            store_orphan_transaction(tx, state);
            return Ok(());
        }
        state.mempool.counters.rejected_total =
            state.mempool.counters.rejected_total.saturating_add(1);
        return Err(err);
    }
    if state.mempool.transactions.len() >= state.mempool.max_transactions {
        state.mempool.counters.pressure_events_total = state
            .mempool
            .counters
            .pressure_events_total
            .saturating_add(1);

        let lowest_txid = lowest_priority_txid(state).ok_or_else(|| {
            PulseError::Internal("mempool pressure detected with no eviction candidate".into())
        })?;
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
            return Err(PulseError::InvalidTransaction(format!(
                "mempool backpressure active (tier={} tx_pressure_bps={} orphan_pressure_bps={} high_bps={} saturated_bps={}): transaction priority below threshold",
                pressure_tier.as_str(),
                tx_pressure_bps,
                orphan_pressure_bps,
                MEMPOOL_PRESSURE_HIGH_BPS,
                MEMPOOL_PRESSURE_SATURATED_BPS
            )));
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
    Ok(())
}

pub fn accept_block(
    block: Block,
    state: &mut ChainState,
    source: AcceptSource,
) -> Result<(), PulseError> {
    validate_block(&block, state)?;

    let enforce_pow = matches!(
        source,
        AcceptSource::Rpc | AcceptSource::P2p | AcceptSource::LocalMining
    );
    let pow = pow_evaluate(&block.header);
    if enforce_pow && !pow.accepted {
        return Err(PulseError::InvalidBlock(format!(
            "pow rejected by current {} policy",
            selected_pow_name()
        )));
    }

    apply_block(&block, state)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        mining::{build_candidate_block, build_coinbase_transaction},
    };

    #[test]
    fn rejects_block_with_invalid_pow() {
        let mut state = init_chain_state("test".to_string());
        let parents = vec![state.dag.genesis_hash.clone()];
        let txs = vec![build_coinbase_transaction("miner1", 50, 1)];
        let mut block = build_candidate_block(parents, 1, u32::MAX, txs);
        block.hash = "bad-pow".to_string();
        block.header.nonce = 0;

        let err = accept_block(block, &mut state, AcceptSource::P2p).unwrap_err();
        assert!(matches!(err, PulseError::InvalidBlock(msg) if msg.contains("pow rejected")));
    }

    #[test]
    fn accepts_block_with_valid_pow() {
        let mut state = init_chain_state("test".to_string());
        let parents = vec![state.dag.genesis_hash.clone()];
        let txs = vec![build_coinbase_transaction("miner1", 50, 1)];
        let mut block = build_candidate_block(parents, 1, 1, txs);
        block.hash = "good-pow".to_string();
        block.header.nonce = 0;

        assert!(accept_block(block, &mut state, AcceptSource::P2p).is_ok());
    }
}
