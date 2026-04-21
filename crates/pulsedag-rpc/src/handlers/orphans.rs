use axum::{extract::State, Json};
use serde::Serialize;

use crate::{api::ApiResponse, api::RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct OrphanEntry {
    pub hash: String,
    pub missing_parents: Vec<String>,
    pub received_at_ms: Option<u64>,
}

#[derive(Debug, serde::Serialize)]
pub struct OrphanStatusData {
    pub orphan_count: usize,
    pub orphans: Vec<OrphanEntry>,
}

pub async fn get_orphans<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<OrphanStatusData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let mut orphans = chain.orphan_blocks.keys().cloned().collect::<Vec<_>>();
    orphans.sort();
    let orphans = orphans.into_iter().map(|hash| OrphanEntry {
        missing_parents: chain.orphan_missing_parents.get(&hash).cloned().unwrap_or_default(),
        received_at_ms: chain.orphan_received_at_ms.get(&hash).copied(),
        hash,
    }).collect::<Vec<_>>();
    Json(ApiResponse::ok(OrphanStatusData { orphan_count: orphans.len(), orphans }))
}
