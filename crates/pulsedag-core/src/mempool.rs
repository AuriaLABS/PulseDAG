use crate::{
    apply::apply_transaction,
    mining::current_ts,
    state::ChainState,
    types::{OutPoint, Transaction},
    validation::validate_transaction,
};

#[derive(Debug, Clone)]
pub struct MempoolReconcileResult {
    pub removed_txids: Vec<String>,
    pub kept_txids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MempoolPolicy {
    pub limit: usize,
    pub fee_floor: u64,
    pub ttl_secs: u64,
}

#[derive(Debug, Clone)]
pub struct MempoolTopItem {
    pub txid: String,
    pub fee: u64,
    pub fee_density: f64,
    pub received_at_unix: u64,
}

pub fn fee_density(tx: &Transaction) -> f64 {
    let weight = (tx.inputs.len() + tx.outputs.len()).max(1) as f64;
    (tx.fee as f64) / weight
}

pub fn mempool_policy(state: &ChainState) -> MempoolPolicy {
    MempoolPolicy {
        limit: state.mempool.limit,
        fee_floor: state.mempool.fee_floor,
        ttl_secs: state.mempool.ttl_secs,
    }
}

pub fn mempool_top(state: &ChainState, limit: usize) -> Vec<MempoolTopItem> {
    let mut items = state
        .mempool
        .transactions
        .values()
        .map(|tx| MempoolTopItem {
            txid: tx.txid.clone(),
            fee: tx.fee,
            fee_density: fee_density(tx),
            received_at_unix: state
                .mempool
                .received_at_unix
                .get(&tx.txid)
                .copied()
                .unwrap_or(0),
        })
        .collect::<Vec<_>>();
    items.sort_by(|a, b| {
        b.fee_density
            .total_cmp(&a.fee_density)
            .then_with(|| b.fee.cmp(&a.fee))
            .then_with(|| a.received_at_unix.cmp(&b.received_at_unix))
            .then_with(|| a.txid.cmp(&b.txid))
    });
    items.truncate(limit);
    items
}

fn remove_tx_from_mempool(state: &mut ChainState, txid: &str) -> Option<Transaction> {
    let tx = state.mempool.transactions.remove(txid)?;
    for input in &tx.inputs {
        state.mempool.spent_outpoints.remove(&input.previous_output);
    }
    state.mempool.received_at_unix.remove(txid);
    state.mempool.tx_sequence.remove(txid);
    Some(tx)
}

pub fn sanitize_mempool(state: &mut ChainState) -> MempoolReconcileResult {
    let now = current_ts();
    let ttl_secs = state.mempool.ttl_secs;
    let stale = state
        .mempool
        .received_at_unix
        .iter()
        .filter(|(_, received_at)| now.saturating_sub(**received_at) > ttl_secs)
        .map(|(txid, _)| txid.clone())
        .collect::<Vec<_>>();
    for txid in stale {
        let _ = remove_tx_from_mempool(state, &txid);
    }
    let mut result = reconcile_mempool(state);
    state.mempool.sanitize_runs = state.mempool.sanitize_runs.saturating_add(1);
    result.kept_txids.sort();
    result.removed_txids.sort();
    result
}

pub fn evict_lowest_fee_density(state: &mut ChainState) -> Option<String> {
    let candidate = state
        .mempool
        .transactions
        .values()
        .map(|tx| {
            let density = fee_density(tx);
            let seq = state
                .mempool
                .tx_sequence
                .get(&tx.txid)
                .copied()
                .unwrap_or(u64::MAX);
            (tx.txid.clone(), density, seq)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.2.cmp(&b.2)));
    if let Some((txid, _, _)) = candidate {
        let _ = remove_tx_from_mempool(state, &txid);
        state.mempool.evicted_total = state.mempool.evicted_total.saturating_add(1);
        return Some(txid);
    }
    None
}

pub fn rebuild_spent_outpoints(state: &mut ChainState) {
    let mut spent = std::collections::HashSet::<OutPoint>::new();
    for tx in state.mempool.transactions.values() {
        for input in &tx.inputs {
            spent.insert(input.previous_output.clone());
        }
    }
    state.mempool.spent_outpoints = spent;
}

pub fn reconcile_mempool(state: &mut ChainState) -> MempoolReconcileResult {
    let tx_count = state.mempool.transactions.len();
    if tx_count == 0 {
        state.mempool.spent_outpoints.clear();
        state.mempool.received_at_unix.clear();
        state.mempool.tx_sequence.clear();
        return MempoolReconcileResult {
            removed_txids: Vec::new(),
            kept_txids: Vec::new(),
        };
    }

    let mut txs = std::mem::take(&mut state.mempool.transactions)
        .into_values()
        .collect::<Vec<_>>();
    txs.sort_by(|a, b| a.txid.cmp(&b.txid));

    let mut working = state.clone();
    working.mempool.transactions.clear();
    working.mempool.spent_outpoints.clear();
    working.mempool.received_at_unix.clear();
    working.mempool.tx_sequence.clear();

    let mut removed_txids = Vec::with_capacity(tx_count);
    let mut kept_txids = Vec::with_capacity(tx_count);

    for tx in txs {
        let txid = tx.txid.clone();
        let valid = validate_transaction(&tx, &working).is_ok()
            && apply_transaction(&tx, &mut working, 0).is_ok();
        if valid {
            working.mempool.transactions.insert(txid.clone(), tx);
            working.mempool.received_at_unix.insert(
                txid.clone(),
                state
                    .mempool
                    .received_at_unix
                    .get(&txid)
                    .copied()
                    .unwrap_or_else(current_ts),
            );
            working.mempool.tx_sequence.insert(
                txid.clone(),
                state
                    .mempool
                    .tx_sequence
                    .get(&txid)
                    .copied()
                    .unwrap_or_else(|| {
                        let seq = working.mempool.next_sequence;
                        working.mempool.next_sequence =
                            working.mempool.next_sequence.saturating_add(1);
                        seq
                    }),
            );
            kept_txids.push(txid);
        } else {
            removed_txids.push(txid);
        }
    }

    state.mempool = working.mempool;

    MempoolReconcileResult {
        removed_txids,
        kept_txids,
    }
}
