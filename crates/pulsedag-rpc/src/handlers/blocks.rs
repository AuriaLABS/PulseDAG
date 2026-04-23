use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{
    extract::{Query, State},
    Json,
};

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
pub struct ListQuery {
    pub limit: Option<usize>,
}

#[derive(Debug, serde::Deserialize)]
pub struct PageQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, serde::Serialize)]
pub struct BlocksData {
    pub count: usize,
    pub blocks: Vec<BlockListItem>,
}

fn bounded_limit(limit: Option<usize>, default: usize, max: usize) -> usize {
    limit.unwrap_or(default).min(max)
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
    blocks.sort_by(|a, b| {
        b.height
            .cmp(&a.height)
            .then_with(|| b.timestamp.cmp(&a.timestamp))
    });
    Json(ApiResponse::ok(BlocksData {
        count: blocks.len(),
        blocks,
    }))
}

pub async fn get_blocks_latest<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<BlockListItem>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    match chain.dag.blocks.values().max_by(|a, b| {
        a.header
            .height
            .cmp(&b.header.height)
            .then_with(|| a.header.timestamp.cmp(&b.header.timestamp))
    }) {
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

pub async fn get_blocks_recent<S: RpcStateLike>(
    State(state): State<S>,
    Query(query): Query<ListQuery>,
) -> Json<ApiResponse<BlocksData>> {
    let limit = bounded_limit(query.limit, 10, 100);
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
    blocks.sort_by(|a, b| {
        b.height
            .cmp(&a.height)
            .then_with(|| b.timestamp.cmp(&a.timestamp))
    });
    blocks.truncate(limit);
    Json(ApiResponse::ok(BlocksData {
        count: blocks.len(),
        blocks,
    }))
}

pub async fn get_blocks_page<S: RpcStateLike>(
    State(state): State<S>,
    Query(query): Query<PageQuery>,
) -> Json<ApiResponse<BlocksData>> {
    let limit = bounded_limit(query.limit, 20, 100);
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
    blocks.sort_by(|a, b| {
        b.height
            .cmp(&a.height)
            .then_with(|| b.timestamp.cmp(&a.timestamp))
    });
    let blocks = blocks
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    Json(ApiResponse::ok(BlocksData {
        count: blocks.len(),
        blocks,
    }))
}

#[cfg(test)]
mod tests {
    use super::bounded_limit;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn limit_normalization_is_capped_and_never_zero_for_defaults(raw in any::<usize>()) {
            let recent = bounded_limit(Some(raw), 10, 100);
            prop_assert!(recent <= 100);

            let page = bounded_limit(Some(raw), 20, 100);
            prop_assert!(page <= 100);
        }
    }

    #[test]
    fn limit_normalization_uses_defaults_when_missing() {
        assert_eq!(bounded_limit(None, 10, 100), 10);
        assert_eq!(bounded_limit(None, 20, 100), 20);
    }
}
