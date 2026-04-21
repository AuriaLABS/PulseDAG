use axum::{extract::State, Json};
use crate::{api::ApiResponse, api::RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct DiagnosticsData {
    pub version: String,
    pub chain_id: String,
    pub best_height: u64,
    pub block_count: usize,
    pub tip_count: usize,
    pub mempool_size: usize,
    pub utxo_count: usize,
    pub snapshot_exists: bool,
    pub p2p_enabled: bool,
    pub peer_count: usize,
}

pub async fn get_diagnostics<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<DiagnosticsData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let snapshot_exists = state.storage().snapshot_exists().unwrap_or(false);
    let (p2p_enabled, peer_count) = match state.p2p() {
        Some(p2p) => match p2p.status() {
            Ok(status) => (true, status.connected_peers.len()),
            Err(_) => (true, 0),
        },
        None => (false, 0),
    };

    Json(ApiResponse::ok(DiagnosticsData {
        version: "v1.1.1".to_string(),
        chain_id: chain.chain_id.clone(),
        best_height: chain.dag.best_height,
        block_count: chain.dag.blocks.len(),
        tip_count: chain.dag.tips.len(),
        mempool_size: chain.mempool.transactions.len(),
        utxo_count: chain.utxo.utxos.len(),
        snapshot_exists,
        p2p_enabled,
        peer_count,
    }))
}
