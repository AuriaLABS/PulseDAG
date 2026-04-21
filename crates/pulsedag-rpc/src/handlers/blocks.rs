use axum::{extract::{Query, State}, Json};
use crate::{api::ApiResponse, api::RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct BlockListItem {
    pub hash: String,
    pub height: u64,
    pub blue_score: u64,
    pub tx_count: usize,
    pub timestamp: u64,
    pub parent_count: usize,
}

#[derive(Debug, serde::Deserialize)]
pub struct ListQuery { pub limit: Option<usize> }

#[derive(Debug, serde::Deserialize)]
pub struct PageQuery { pub limit: Option<usize>, pub offset: Option<usize> }

#[derive(Debug, serde::Serialize)]
pub struct BlocksData {
    pub count: usize,
    pub blocks: Vec<BlockListItem>,
}

pub async fn get_blocks<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<BlocksData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let mut blocks = chain
        .dag
        .blocks
        .values()
        .map(|b| BlockListItem {
            hash: b.hash.clone(),
            height: b.header.height,
            blue_score: b.header.blue_score,
            tx_count: b.transactions.len(),
            timestamp: b.header.timestamp,
            parent_count: b.header.parents.len(),
        })
        .collect::<Vec<_>>();
    blocks.sort_by(|a, b| b.height.cmp(&a.height).then_with(|| b.timestamp.cmp(&a.timestamp)));
    Json(ApiResponse::ok(BlocksData { count: blocks.len(), blocks }))
}

pub async fn get_blocks_latest<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<BlockListItem>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    match chain
        .dag
        .blocks
        .values()
        .max_by(|a, b| a.header.height.cmp(&b.header.height).then_with(|| a.header.timestamp.cmp(&b.header.timestamp)))
    {
        Some(b) => Json(ApiResponse::ok(BlockListItem {
            hash: b.hash.clone(),
            height: b.header.height,
            blue_score: b.header.blue_score,
            tx_count: b.transactions.len(),
            timestamp: b.header.timestamp,
            parent_count: b.header.parents.len(),
        })),
        None => Json(ApiResponse::err("NOT_FOUND", "no blocks found")),
    }
}


pub async fn get_blocks_recent<S: RpcStateLike>(State(state): State<S>, Query(query): Query<ListQuery>) -> Json<ApiResponse<BlocksData>> {
    let limit = query.limit.unwrap_or(10).min(100);
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let mut blocks = chain
        .dag
        .blocks
        .values()
        .map(|b| BlockListItem {
            hash: b.hash.clone(),
            height: b.header.height,
            blue_score: b.header.blue_score,
            tx_count: b.transactions.len(),
            timestamp: b.header.timestamp,
            parent_count: b.header.parents.len(),
        })
        .collect::<Vec<_>>();
    blocks.sort_by(|a, b| b.height.cmp(&a.height).then_with(|| b.timestamp.cmp(&a.timestamp)));
    blocks.truncate(limit);
    Json(ApiResponse::ok(BlocksData { count: blocks.len(), blocks }))
}


pub async fn get_blocks_page<S: RpcStateLike>(State(state): State<S>, Query(query): Query<PageQuery>) -> Json<ApiResponse<BlocksData>> {
    let limit = query.limit.unwrap_or(20).min(100);
    let offset = query.offset.unwrap_or(0);
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let mut blocks = chain
        .dag
        .blocks
        .values()
        .map(|b| BlockListItem {
            hash: b.hash.clone(),
            height: b.header.height,
            blue_score: b.header.blue_score,
            tx_count: b.transactions.len(),
            timestamp: b.header.timestamp,
            parent_count: b.header.parents.len(),
        })
        .collect::<Vec<_>>();
    blocks.sort_by(|a, b| b.height.cmp(&a.height).then_with(|| b.timestamp.cmp(&a.timestamp)));
    let blocks = blocks.into_iter().skip(offset).take(limit).collect::<Vec<_>>();
    Json(ApiResponse::ok(BlocksData { count: blocks.len(), blocks }))
}
