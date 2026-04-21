use axum::{extract::State, Json};
use crate::{api::ApiResponse, api::RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct SnapshotInfoData {
    pub snapshot_exists: bool,
    pub chain_id: Option<String>,
    pub best_height: Option<u64>,
    pub block_count: Option<usize>,
    pub tip_count: Option<usize>,
    pub utxo_count: Option<usize>,
    pub mempool_size: Option<usize>,
    pub persisted_block_count: usize,
}

pub async fn get_snapshot_info<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<SnapshotInfoData>> {
    let persisted_block_count = match state.storage().list_blocks() {
        Ok(v) => v.len(),
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    match state.storage().load_chain_state() {
        Ok(Some(snapshot)) => Json(ApiResponse::ok(SnapshotInfoData {
            snapshot_exists: true,
            chain_id: Some(snapshot.chain_id),
            best_height: Some(snapshot.dag.best_height),
            block_count: Some(snapshot.dag.blocks.len()),
            tip_count: Some(snapshot.dag.tips.len()),
            utxo_count: Some(snapshot.utxo.utxos.len()),
            mempool_size: Some(snapshot.mempool.transactions.len()),
            persisted_block_count,
        })),
        Ok(None) => Json(ApiResponse::ok(SnapshotInfoData {
            snapshot_exists: false,
            chain_id: None,
            best_height: None,
            block_count: None,
            tip_count: None,
            utxo_count: None,
            mempool_size: None,
            persisted_block_count,
        })),
        Err(e) => Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    }
}
