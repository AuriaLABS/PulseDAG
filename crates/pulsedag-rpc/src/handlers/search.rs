use axum::{extract::{Path, State}, Json};
use crate::{api::ApiResponse, api::RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct SearchResultData {
    pub query: String,
    pub kind: String,
    pub found: bool,
    pub hash: Option<String>,
    pub address: Option<String>,
    pub block_height: Option<u64>,
    pub status: Option<String>,
}

pub async fn get_search<S: RpcStateLike>(State(state): State<S>, Path(query): Path<String>) -> Json<ApiResponse<SearchResultData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;

    if let Some(block) = chain.dag.blocks.get(&query) {
        return Json(ApiResponse::ok(SearchResultData {
            query,
            kind: "block".into(),
            found: true,
            hash: Some(block.hash.clone()),
            address: None,
            block_height: Some(block.header.height),
            status: Some("confirmed".into()),
        }));
    }

    if chain.utxo.address_index.contains_key(&query) {
        return Json(ApiResponse::ok(SearchResultData {
            query: query.clone(),
            kind: "address".into(),
            found: true,
            hash: None,
            address: Some(query),
            block_height: None,
            status: Some("known".into()),
        }));
    }

    if chain.mempool.transactions.contains_key(&query) {
        return Json(ApiResponse::ok(SearchResultData {
            query: query.clone(),
            kind: "transaction".into(),
            found: true,
            hash: Some(query),
            address: None,
            block_height: None,
            status: Some("mempool".into()),
        }));
    }

    for block in chain.dag.blocks.values() {
        if let Some(tx) = block.transactions.iter().find(|t| t.txid == query) {
            return Json(ApiResponse::ok(SearchResultData {
                query: query.clone(),
                kind: "transaction".into(),
                found: true,
                hash: Some(tx.txid.clone()),
                address: None,
                block_height: Some(block.header.height),
                status: Some("confirmed".into()),
            }));
        }
    }

    Json(ApiResponse::ok(SearchResultData {
        query,
        kind: "unknown".into(),
        found: false,
        hash: None,
        address: None,
        block_height: None,
        status: None,
    }))
}
