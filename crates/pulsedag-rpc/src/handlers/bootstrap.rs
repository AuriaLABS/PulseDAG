use axum::{extract::State, Json};
use crate::{api::ApiResponse, api::RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct BootstrapData {
    pub p2p_enabled: bool,
    pub peer_count: usize,
    pub snapshot_exists: bool,
    pub persisted_block_count: usize,
    pub best_height: u64,
    pub bootstrap_ready: bool,
    pub notes: Vec<String>,
}

pub async fn get_bootstrap_status<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<BootstrapData>> {
    let persisted_block_count = match state.storage().list_blocks() {
        Ok(v) => v.len(),
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };
    let snapshot_exists = state.storage().snapshot_exists().unwrap_or(false);
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;

    let (p2p_enabled, peer_count) = match state.p2p() {
        Some(p2p) => match p2p.status() {
            Ok(status) => (true, status.connected_peers.len()),
            Err(_) => (true, 0),
        },
        None => (false, 0),
    };

    let mut notes = Vec::new();
    if !p2p_enabled {
        notes.push("p2p disabled; bootstrap limited to local recovery".to_string());
    }
    if snapshot_exists {
        notes.push("snapshot available for faster startup".to_string());
    }
    if persisted_block_count == 0 {
        notes.push("no persisted blocks yet".to_string());
    }
    if peer_count == 0 {
        notes.push("no connected peers visible".to_string());
    }

    let bootstrap_ready = snapshot_exists || persisted_block_count > 0 || peer_count > 0;

    Json(ApiResponse::ok(BootstrapData {
        p2p_enabled,
        peer_count,
        snapshot_exists,
        persisted_block_count,
        best_height: chain.dag.best_height,
        bootstrap_ready,
        notes,
    }))
}
