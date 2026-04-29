use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{extract::State, Json};

#[derive(Debug, serde::Serialize)]
pub struct RebuildPreviewData {
    pub can_rebuild: bool,
    pub persisted_block_count: usize,
    pub highest_persisted_height: Option<u64>,
    pub snapshot_exists: bool,
    pub reasons: Vec<String>,
}

pub async fn get_rebuild_preview<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<RebuildPreviewData>> {
    let persisted_blocks = match state.storage().list_blocks() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let snapshot_exists = state.storage().snapshot_exists().unwrap_or(false);
    let highest_persisted_height = persisted_blocks.iter().map(|b| b.header.height).max();
    let mut reasons = Vec::new();

    if persisted_blocks.is_empty() {
        reasons.push("no persisted blocks found".to_string());
    }
    if !snapshot_exists {
        reasons.push("snapshot missing; rebuild may rely entirely on persisted blocks".to_string());
    }

    let can_rebuild = !persisted_blocks.is_empty();
    Json(ApiResponse::ok(RebuildPreviewData {
        can_rebuild,
        persisted_block_count: persisted_blocks.len(),
        highest_persisted_height,
        snapshot_exists,
        reasons,
    }))
}
