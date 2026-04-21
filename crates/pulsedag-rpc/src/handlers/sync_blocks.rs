use axum::{extract::State, Json};
use crate::{api::ApiResponse, api::RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct PersistedBlockItem {
    pub hash: String,
    pub height: u64,
    pub tx_count: usize,
    pub timestamp: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct PersistedBlocksData {
    pub count: usize,
    pub blocks: Vec<PersistedBlockItem>,
}

pub async fn get_sync_blocks<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<PersistedBlocksData>> {
    let blocks = match state.storage().list_blocks() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let mut items = blocks
        .into_iter()
        .map(|b| PersistedBlockItem {
            hash: b.hash,
            height: b.header.height,
            tx_count: b.transactions.len(),
            timestamp: b.header.timestamp,
        })
        .collect::<Vec<_>>();
    items.sort_by(|a, b| b.height.cmp(&a.height).then_with(|| b.timestamp.cmp(&a.timestamp)));

    Json(ApiResponse::ok(PersistedBlocksData {
        count: items.len(),
        blocks: items,
    }))
}
