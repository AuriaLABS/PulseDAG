use axum::{extract::State, Json};
use crate::{api::ApiResponse, api::RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct IncrementalSyncData {
    pub local_best_height: u64,
    pub snapshot_height: Option<u64>,
    pub highest_persisted_height: Option<u64>,
    pub start_height: u64,
    pub target_height: u64,
    pub gap: u64,
    pub can_incremental_sync: bool,
}

pub async fn get_incremental_sync_plan<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<IncrementalSyncData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let local_best_height = chain.dag.best_height;
    drop(chain);

    let snapshot_height = match state.storage().load_chain_state() {
        Ok(Some(snapshot)) => Some(snapshot.dag.best_height),
        Ok(None) => None,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let persisted_blocks = match state.storage().list_blocks() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };
    let highest_persisted_height = persisted_blocks.iter().map(|b| b.header.height).max();

    let start_height = snapshot_height.unwrap_or(0).max(local_best_height);
    let target_height = highest_persisted_height.unwrap_or(local_best_height).max(local_best_height);
    let gap = target_height.saturating_sub(start_height);
    let can_incremental_sync = highest_persisted_height.is_some() || state.p2p().is_some();

    Json(ApiResponse::ok(IncrementalSyncData {
        local_best_height,
        snapshot_height,
        highest_persisted_height,
        start_height,
        target_height,
        gap,
        can_incremental_sync,
    }))
}
