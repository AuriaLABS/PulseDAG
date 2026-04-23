use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{extract::State, Json};

#[derive(Debug, serde::Serialize)]
pub struct ReplayPlanData {
    pub snapshot_exists: bool,
    pub snapshot_height: Option<u64>,
    pub persisted_block_count: usize,
    pub highest_persisted_height: Option<u64>,
    pub target_height: u64,
    pub needs_rebuild: bool,
    pub recommended_action: String,
    pub startup_path: String,
    pub startup_fastboot_used: bool,
    pub startup_replay_required: bool,
    pub startup_fallback_reason: Option<String>,
}

pub async fn get_replay_plan<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<ReplayPlanData>> {
    let persisted_blocks = match state.storage().list_blocks() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };
    let highest_persisted_height = persisted_blocks.iter().map(|b| b.header.height).max();
    let persisted_block_count = persisted_blocks.len();

    let snapshot = match state.storage().load_chain_state() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let target_height = chain
        .dag
        .best_height
        .max(highest_persisted_height.unwrap_or(0));
    let runtime_handle = state.runtime();
    let runtime = runtime_handle.read().await;

    let (snapshot_exists, snapshot_height) = match snapshot {
        Some(s) => (true, Some(s.dag.best_height)),
        None => (false, None),
    };

    let needs_rebuild = !snapshot_exists
        || snapshot_height.unwrap_or(0) < highest_persisted_height.unwrap_or(0)
        || chain.dag.blocks.len() != persisted_block_count;

    let recommended_action = if !snapshot_exists {
        "create snapshot or rebuild from persisted blocks".to_string()
    } else if snapshot_height.unwrap_or(0) < highest_persisted_height.unwrap_or(0) {
        "replay persisted blocks above snapshot height".to_string()
    } else if chain.dag.blocks.len() != persisted_block_count {
        "run sync verify and consider rebuild".to_string()
    } else {
        "replay state looks aligned".to_string()
    };

    Json(ApiResponse::ok(ReplayPlanData {
        snapshot_exists,
        snapshot_height,
        persisted_block_count,
        highest_persisted_height,
        target_height,
        needs_rebuild,
        recommended_action,
        startup_path: runtime.startup_path.clone(),
        startup_fastboot_used: runtime.startup_fastboot_used,
        startup_replay_required: runtime.startup_replay_required,
        startup_fallback_reason: runtime.startup_fallback_reason.clone(),
    }))
}
