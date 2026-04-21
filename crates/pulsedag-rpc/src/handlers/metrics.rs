use axum::{extract::State, Json};

use crate::{api::ApiResponse, api::RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct MetricsData {
    pub chain_id: String,
    pub best_height: u64,
    pub block_count: usize,
    pub tip_count: usize,
    pub mempool_size: usize,
    pub utxo_count: usize,
    pub address_count: usize,
    pub circulating_supply: u64,
    pub last_block_hash: Option<String>,
    pub target_block_interval_secs: u64,
    pub window_size: usize,
    pub retarget_multiplier_bps: u64,
    pub suggested_difficulty: u64,
}

pub async fn get_metrics<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<MetricsData>> {
    let chain = state.chain().read().await;
    let snapshot = pulsedag_core::dev_difficulty_snapshot(&chain);
    let circulating_supply = chain.utxo.utxos.values().map(|u| u.amount).sum();
    let last_block_hash = chain
        .dag
        .blocks
        .values()
        .max_by_key(|b| b.header.height)
        .map(|b| b.hash.clone());

    Json(ApiResponse::ok(MetricsData {
        chain_id: chain.chain_id.clone(),
        best_height: chain.dag.best_height,
        block_count: chain.dag.blocks.len(),
        tip_count: chain.dag.tips.len(),
        mempool_size: chain.mempool.transactions.len(),
        utxo_count: chain.utxo.utxos.len(),
        address_count: chain.utxo.address_index.len(),
        circulating_supply,
        last_block_hash,
        target_block_interval_secs: snapshot.policy.target_block_interval_secs,
        window_size: snapshot.policy.window_size,
        retarget_multiplier_bps: snapshot.retarget_multiplier_bps,
        suggested_difficulty: snapshot.suggested_difficulty,
    }))
}
