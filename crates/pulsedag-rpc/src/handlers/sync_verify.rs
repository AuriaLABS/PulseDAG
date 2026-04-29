use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{extract::State, Json};
use std::collections::BTreeSet;

#[derive(Debug, serde::Serialize)]
pub struct SyncVerifyData {
    pub in_memory_block_count: usize,
    pub persisted_block_count: usize,
    pub missing_in_storage: Vec<String>,
    pub missing_in_memory: Vec<String>,
    pub consistent: bool,
}

pub async fn get_sync_verify<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<SyncVerifyData>> {
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
    let memory_hashes = chain.dag.blocks.keys().cloned().collect::<BTreeSet<_>>();

    let missing_in_storage = memory_hashes
        .difference(&persisted_hashes)
        .cloned()
        .collect::<Vec<_>>();
    let missing_in_memory = persisted_hashes
        .difference(&memory_hashes)
        .cloned()
        .collect::<Vec<_>>();
    let consistent = missing_in_storage.is_empty() && missing_in_memory.is_empty();

    Json(ApiResponse::ok(SyncVerifyData {
        in_memory_block_count: memory_hashes.len(),
        persisted_block_count: persisted_hashes.len(),
        missing_in_storage,
        missing_in_memory,
        consistent,
    }))
}
