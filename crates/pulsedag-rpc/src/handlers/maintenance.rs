use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{extract::State, Json};
use std::collections::BTreeSet;

#[derive(Debug, serde::Serialize)]
pub struct MaintenanceReportData {
    pub snapshot_exists: bool,
    pub snapshot_height: Option<u64>,
    pub captured_at_unix: Option<u64>,
    pub best_height: u64,
    pub in_memory_block_count: usize,
    pub persisted_block_count: usize,
    pub recommended_keep_from_height: u64,
    pub consistent: bool,
    pub recommended_action: String,
}

pub async fn get_maintenance_report<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<MaintenanceReportData>> {
    let snapshot_exists = match state.storage().snapshot_exists() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };
    let persisted_blocks = match state.storage().list_blocks() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };
    let persisted_hashes = persisted_blocks
        .into_iter()
        .map(|b| b.hash)
        .collect::<BTreeSet<_>>();

    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let runtime = state.runtime().read().await;
    let keep_recent = runtime.prune_keep_recent_blocks.max(1);
    let recommended_keep_from_height = chain
        .dag
        .best_height
        .saturating_sub(keep_recent.saturating_sub(1));
    let captured_at_unix = match state.storage().snapshot_captured_at_unix() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };
    let memory_hashes = chain.dag.blocks.keys().cloned().collect::<BTreeSet<_>>();

    let consistent = memory_hashes == persisted_hashes;
    let recommended_action = if !snapshot_exists {
        "create or refresh snapshot soon".to_string()
    } else if !consistent {
        "run sync verify and consider rebuild with force=true".to_string()
    } else if chain.mempool.transactions.len() > 1000 {
        "inspect mempool pressure".to_string()
    } else {
        "node state looks healthy".to_string()
    };

    Json(ApiResponse::ok(MaintenanceReportData {
        snapshot_exists,
        snapshot_height: if snapshot_exists {
            Some(chain.dag.best_height)
        } else {
            None
        },
        captured_at_unix,
        best_height: chain.dag.best_height,
        in_memory_block_count: memory_hashes.len(),
        persisted_block_count: persisted_hashes.len(),
        recommended_keep_from_height,
        consistent,
        recommended_action,
    }))
}
