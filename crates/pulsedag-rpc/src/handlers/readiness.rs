use std::collections::BTreeSet;
use axum::{extract::State, Json};
use crate::{api::ApiResponse, api::RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct ReadinessData {
    pub ready_for_release: bool,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
}

pub async fn get_readiness<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<ReadinessData>> {
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();

    let snapshot_exists = match state.storage().snapshot_exists() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let persisted_blocks = match state.storage().list_blocks() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };
    let persisted_hashes = persisted_blocks.iter().map(|b| b.hash.clone()).collect::<BTreeSet<_>>();

    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let memory_hashes = chain.dag.blocks.keys().cloned().collect::<BTreeSet<_>>();

    if !snapshot_exists {
        warnings.push("snapshot is missing".to_string());
    }
    if memory_hashes != persisted_hashes {
        blockers.push("memory and persisted blocks are not aligned".to_string());
    }
    if chain.dag.tips.is_empty() {
        blockers.push("no active tips in dag".to_string());
    }
    if !chain.dag.blocks.contains_key(&chain.dag.genesis_hash) {
        blockers.push("genesis block missing from in-memory dag".to_string());
    }
    if state.p2p().is_none() {
        warnings.push("p2p is disabled".to_string());
    }
    if chain.mempool.transactions.len() > 1000 {
        warnings.push("mempool is large; inspect pressure before release".to_string());
    }

    let ready_for_release = blockers.is_empty();
    Json(ApiResponse::ok(ReadinessData { ready_for_release, blockers, warnings }))
}
