use crate::{apply::apply_transaction, state::ChainState, validation::validate_transaction};

#[derive(Debug, Clone)]
pub struct MempoolReconcileResult {
    pub removed_txids: Vec<String>,
    pub kept_txids: Vec<String>,
}

pub fn reconcile_mempool(state: &mut ChainState) -> MempoolReconcileResult {
    let tx_count = state.mempool.transactions.len();
    if tx_count == 0 {
        state.mempool.spent_outpoints.clear();
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

    let mut removed_txids = Vec::with_capacity(tx_count);
    let mut kept_txids = Vec::with_capacity(tx_count);

    for tx in txs {
        let txid = tx.txid.clone();
        let valid = validate_transaction(&tx, &working).is_ok()
            && apply_transaction(&tx, &mut working, 0).is_ok();
        if valid {
            working.mempool.transactions.insert(txid.clone(), tx);
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
