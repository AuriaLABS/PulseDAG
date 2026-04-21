use crate::api::{ApiResponse, RpcStateLike, SubmitTxRequest};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use pulsedag_wallet::{build_transaction, BuildTxRequest};

#[derive(Debug, serde::Serialize)]
pub struct TxListItem {
    pub txid: String,
    pub fee: u64,
    pub inputs: usize,
    pub outputs: usize,
}
#[derive(Debug, serde::Serialize)]
pub struct TxListData {
    pub count: usize,
    pub transactions: Vec<TxListItem>,
}

#[derive(Debug, serde::Deserialize)]
pub struct TxsQuery {
    pub limit: Option<usize>,
}

#[derive(Debug, serde::Deserialize)]
pub struct TxsPageQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, serde::Serialize)]
pub struct MempoolData {
    pub transaction_count: usize,
    pub spent_outpoints_count: usize,
    pub txids: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct MempoolStatusData {
    pub mempool_size: usize,
    pub mempool_limit: usize,
    pub fee_floor: u64,
    pub ttl_secs: u64,
    pub mempool_evicted_total: u64,
    pub mempool_rejected_total: u64,
    pub mempool_rejected_fee_floor_total: u64,
    pub mempool_sanitize_runs: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct MempoolPolicyData {
    pub mempool_limit: usize,
    pub fee_floor: u64,
    pub ttl_secs: u64,
}

#[derive(Debug, serde::Deserialize)]
pub struct MempoolTopQuery {
    pub limit: Option<usize>,
}

#[derive(Debug, serde::Serialize)]
pub struct MempoolTopData {
    pub count: usize,
    pub transactions: Vec<MempoolTopItem>,
}

#[derive(Debug, serde::Serialize)]
pub struct MempoolTopItem {
    pub txid: String,
    pub fee: u64,
    pub fee_density: f64,
    pub received_at_unix: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct MempoolSanitizeData {
    pub before_count: usize,
    pub after_count: usize,
    pub removed_count: usize,
    pub removed_txids: Vec<String>,
    pub kept_count: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct TxValidateData {
    pub valid: bool,
    pub txid: String,
    pub reason: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct TxDetailData {
    pub txid: String,
    pub fee: u64,
    pub inputs: usize,
    pub outputs: usize,
    pub status: String,
    pub block_hash: Option<String>,
    pub block_height: Option<u64>,
}

pub async fn get_txs_recent<S: RpcStateLike>(
    State(state): State<S>,
    Query(query): Query<TxsQuery>,
) -> Json<ApiResponse<TxListData>> {
    let limit = query.limit.unwrap_or(10).min(100);
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let mut transactions = chain
        .mempool
        .transactions
        .values()
        .map(|tx| TxListItem {
            txid: tx.txid.clone(),
            fee: tx.fee,
            inputs: tx.inputs.len(),
            outputs: tx.outputs.len(),
        })
        .collect::<Vec<_>>();
    transactions.sort_by(|a, b| b.fee.cmp(&a.fee).then_with(|| a.txid.cmp(&b.txid)));
    transactions.truncate(limit);
    Json(ApiResponse::ok(TxListData {
        count: transactions.len(),
        transactions,
    }))
}

pub async fn get_txs<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<TxListData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let transactions = chain
        .mempool
        .transactions
        .values()
        .map(|tx| TxListItem {
            txid: tx.txid.clone(),
            fee: tx.fee,
            inputs: tx.inputs.len(),
            outputs: tx.outputs.len(),
        })
        .collect::<Vec<_>>();
    Json(ApiResponse::ok(TxListData {
        count: transactions.len(),
        transactions,
    }))
}

pub async fn get_mempool<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<MempoolData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let mut txids = chain
        .mempool
        .transactions
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    txids.sort();
    Json(ApiResponse::ok(MempoolData {
        transaction_count: chain.mempool.transactions.len(),
        spent_outpoints_count: chain.mempool.spent_outpoints.len(),
        txids,
    }))
}

pub async fn get_mempool_status<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<MempoolStatusData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    Json(ApiResponse::ok(MempoolStatusData {
        mempool_size: chain.mempool.transactions.len(),
        mempool_limit: chain.mempool.limit,
        fee_floor: chain.mempool.fee_floor,
        ttl_secs: chain.mempool.ttl_secs,
        mempool_evicted_total: chain.mempool.evicted_total,
        mempool_rejected_total: chain.mempool.rejected_total,
        mempool_rejected_fee_floor_total: chain.mempool.rejected_fee_floor_total,
        mempool_sanitize_runs: chain.mempool.sanitize_runs,
    }))
}

pub async fn get_mempool_policy<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<MempoolPolicyData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    Json(ApiResponse::ok(MempoolPolicyData {
        mempool_limit: chain.mempool.limit,
        fee_floor: chain.mempool.fee_floor,
        ttl_secs: chain.mempool.ttl_secs,
    }))
}

pub async fn get_mempool_top<S: RpcStateLike>(
    State(state): State<S>,
    Query(query): Query<MempoolTopQuery>,
) -> Json<ApiResponse<MempoolTopData>> {
    let limit = query.limit.unwrap_or(20).min(200);
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let top = pulsedag_core::mempool_top(&chain, limit)
        .into_iter()
        .map(|item| MempoolTopItem {
            txid: item.txid,
            fee: item.fee,
            fee_density: item.fee_density,
            received_at_unix: item.received_at_unix,
        })
        .collect::<Vec<_>>();
    Json(ApiResponse::ok(MempoolTopData {
        count: top.len(),
        transactions: top,
    }))
}

pub async fn post_mempool_sanitize<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<MempoolSanitizeData>> {
    let chain_handle = state.chain();
    let mut chain = chain_handle.write().await;
    let before_count = chain.mempool.transactions.len();
    let result = pulsedag_core::sanitize_mempool(&mut chain);
    let after_count = chain.mempool.transactions.len();
    let snapshot = chain.clone();
    drop(chain);
    if let Err(e) = state.storage().persist_chain_state(&snapshot) {
        return Json(ApiResponse::err("STORAGE_ERROR", e.to_string()));
    }
    Json(ApiResponse::ok(MempoolSanitizeData {
        before_count,
        after_count,
        removed_count: result.removed_txids.len(),
        removed_txids: result.removed_txids,
        kept_count: result.kept_txids.len(),
    }))
}

pub async fn get_tx<S: RpcStateLike>(
    State(state): State<S>,
    Path(txid): Path<String>,
) -> Json<ApiResponse<TxDetailData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;

    if let Some(tx) = chain.mempool.transactions.get(&txid) {
        return Json(ApiResponse::ok(TxDetailData {
            txid: tx.txid.clone(),
            fee: tx.fee,
            inputs: tx.inputs.len(),
            outputs: tx.outputs.len(),
            status: "mempool".into(),
            block_hash: None,
            block_height: None,
        }));
    }

    for block in chain.dag.blocks.values() {
        if let Some(tx) = block.transactions.iter().find(|t| t.txid == txid) {
            return Json(ApiResponse::ok(TxDetailData {
                txid: tx.txid.clone(),
                fee: tx.fee,
                inputs: tx.inputs.len(),
                outputs: tx.outputs.len(),
                status: "confirmed".into(),
                block_hash: Some(block.hash.clone()),
                block_height: Some(block.header.height),
            }));
        }
    }

    Json(ApiResponse::err(
        "TX_NOT_FOUND",
        format!("transaction {txid} not found"),
    ))
}

pub async fn post_tx_build<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<BuildTxRequest>,
) -> Json<ApiResponse<pulsedag_wallet::BuildTxResponse>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let available = chain
        .utxo
        .address_index
        .get(&req.from)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|op| chain.utxo.utxos.get(&op).cloned())
        .collect::<Vec<_>>();
    match build_transaction(&req.from, &req.to, req.amount, req.fee, &available, 1) {
        Ok(data) => Json(ApiResponse::ok(data)),
        Err(e) => Json(ApiResponse::err("BUILD_ERROR", e.to_string())),
    }
}

pub async fn post_tx_validate<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<SubmitTxRequest>,
) -> Json<ApiResponse<TxValidateData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let mut simulated = chain.clone();
    drop(chain);

    match pulsedag_core::accept_transaction(
        req.transaction.clone(),
        &mut simulated,
        pulsedag_core::AcceptSource::Rpc,
    ) {
        Ok(_) => Json(ApiResponse::ok(TxValidateData {
            valid: true,
            txid: req.transaction.txid,
            reason: None,
        })),
        Err(e) => Json(ApiResponse::ok(TxValidateData {
            valid: false,
            txid: req.transaction.txid,
            reason: Some(e.to_string()),
        })),
    }
}

pub async fn post_tx_submit<S: RpcStateLike>(
    State(state): State<S>,
    Json(req): Json<SubmitTxRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let chain_handle = state.chain();
    let mut chain = chain_handle.write().await;
    match pulsedag_core::accept_transaction(
        req.transaction.clone(),
        &mut chain,
        pulsedag_core::AcceptSource::Rpc,
    ) {
        Ok(_) => {
            let mempool_size = chain.mempool.transactions.len();
            let snapshot = chain.clone();
            drop(chain);
            if let Err(e) = state.storage().persist_chain_state(&snapshot) {
                return Json(ApiResponse::err("STORAGE_ERROR", e.to_string()));
            }
            if let Some(p2p) = state.p2p() {
                let _ = p2p.broadcast_transaction(&req.transaction);
            }
            Json(ApiResponse::ok(
                serde_json::json!({"accepted": true, "txid": req.transaction.txid, "mempool_size": mempool_size}),
            ))
        }
        Err(e) => Json(ApiResponse::err("TX_REJECTED", e.to_string())),
    }
}

pub async fn get_txs_page<S: RpcStateLike>(
    State(state): State<S>,
    Query(query): Query<TxsPageQuery>,
) -> Json<ApiResponse<TxListData>> {
    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.offset.unwrap_or(0);
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let mut transactions = chain
        .mempool
        .transactions
        .values()
        .map(|tx| TxListItem {
            txid: tx.txid.clone(),
            fee: tx.fee,
            inputs: tx.inputs.len(),
            outputs: tx.outputs.len(),
        })
        .collect::<Vec<_>>();
    transactions.sort_by(|a, b| b.fee.cmp(&a.fee).then_with(|| a.txid.cmp(&b.txid)));
    let transactions = transactions
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    Json(ApiResponse::ok(TxListData {
        count: transactions.len(),
        transactions,
    }))
}
