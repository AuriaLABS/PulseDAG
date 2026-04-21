use axum::{extract::State, Json};
use crate::{api::ApiResponse, api::RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct ConfirmedTxListItem {
    pub txid: String,
    pub fee: u64,
    pub inputs: usize,
    pub outputs: usize,
    pub block_hash: String,
    pub block_height: u64,
    pub status: String,
}

#[derive(Debug, serde::Serialize)]
pub struct ConfirmedTxsData {
    pub count: usize,
    pub transactions: Vec<ConfirmedTxListItem>,
}

pub async fn get_confirmed_transactions<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<ConfirmedTxsData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;

    let mut transactions = Vec::new();
    let mut blocks = chain.dag.blocks.values().collect::<Vec<_>>();
    blocks.sort_by(|a, b| b.header.height.cmp(&a.header.height));

    for block in blocks {
        for tx in &block.transactions {
            transactions.push(ConfirmedTxListItem {
                txid: tx.txid.clone(),
                fee: tx.fee,
                inputs: tx.inputs.len(),
                outputs: tx.outputs.len(),
                block_hash: block.hash.clone(),
                block_height: block.header.height,
                status: "confirmed".into(),
            });
        }
    }

    Json(ApiResponse::ok(ConfirmedTxsData {
        count: transactions.len(),
        transactions,
    }))
}
