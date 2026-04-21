use axum::{extract::State, Json};
use crate::{api::ApiResponse, api::RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct DashboardBlockItem {
    pub hash: String,
    pub height: u64,
    pub tx_count: usize,
    pub timestamp: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct DashboardTxItem {
    pub txid: String,
    pub fee: u64,
    pub inputs: usize,
    pub outputs: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct DashboardSummary {
    pub chain_id: String,
    pub best_height: u64,
    pub block_count: usize,
    pub tip_count: usize,
    pub mempool_size: usize,
    pub utxo_count: usize,
    pub address_count: usize,
    pub circulating_supply: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct DashboardData {
    pub summary: DashboardSummary,
    pub latest_blocks: Vec<DashboardBlockItem>,
    pub mempool_transactions: Vec<DashboardTxItem>,
}

pub async fn get_dashboard<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<DashboardData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;

    let mut latest_blocks = chain.dag.blocks.values().map(|b| DashboardBlockItem {
        hash: b.hash.clone(),
        height: b.header.height,
        tx_count: b.transactions.len(),
        timestamp: b.header.timestamp,
    }).collect::<Vec<_>>();
    latest_blocks.sort_by(|a, b| b.height.cmp(&a.height).then_with(|| b.timestamp.cmp(&a.timestamp)));
    latest_blocks.truncate(10);

    let mut mempool_transactions = chain.mempool.transactions.values().map(|tx| DashboardTxItem {
        txid: tx.txid.clone(),
        fee: tx.fee,
        inputs: tx.inputs.len(),
        outputs: tx.outputs.len(),
    }).collect::<Vec<_>>();
    mempool_transactions.sort_by(|a, b| b.fee.cmp(&a.fee));

    Json(ApiResponse::ok(DashboardData {
        summary: DashboardSummary {
            chain_id: chain.chain_id.clone(),
            best_height: chain.dag.best_height,
            block_count: chain.dag.blocks.len(),
            tip_count: chain.dag.tips.len(),
            mempool_size: chain.mempool.transactions.len(),
            utxo_count: chain.utxo.utxos.len(),
            address_count: chain.utxo.address_index.len(),
            circulating_supply: chain.utxo.utxos.values().map(|u| u.amount).sum(),
        },
        latest_blocks,
        mempool_transactions,
    }))
}
