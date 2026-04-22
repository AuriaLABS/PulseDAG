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
    let runtime_handle = state.runtime();
    let mempool_max_txs = runtime_handle.read().await.mempool_max_txs;
    if chain.mempool.transactions.len() >= mempool_max_txs {
        drop(chain);
        {
            let mut runtime = runtime_handle.write().await;
            runtime.rejected_rpc_txs_mempool_full += 1;
        }
        let _ = state.storage().append_runtime_event(
            "warn",
            "tx_rejected_mempool_full",
            &format!(
                "source=rpc txid={} mempool_max_txs={}",
                req.transaction.txid, mempool_max_txs
            ),
        );
        return Json(ApiResponse::err(
            "MEMPOOL_FULL",
            format!(
                "transaction rejected because mempool reached capacity ({})",
                mempool_max_txs
            ),
        ));
    }
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
