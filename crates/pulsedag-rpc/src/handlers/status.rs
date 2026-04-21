use crate::{api::ApiResponse, api::RpcStateLike};
use axum::{extract::State, Json};

#[derive(Debug, serde::Serialize)]
pub struct NodeStatusData {
    pub service: String,
    pub version: String,
    pub chain_id: String,
    pub best_height: u64,
    pub block_count: usize,
    pub tip_count: usize,
    pub mempool_size: usize,
    pub utxo_count: usize,
    pub address_count: usize,
    pub snapshot_exists: bool,
    pub persisted_block_count: usize,
    pub p2p_enabled: bool,
    pub peer_count: usize,
    pub last_block_hash: Option<String>,
    pub contracts_prepared: bool,
    pub contracts_enabled: bool,
    pub contracts_vm_version: String,
}

fn repo_version() -> String {
    include_str!("../../../../VERSION").trim().to_string()
}

pub async fn get_status<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<NodeStatusData>> {
    let snapshot_exists = match state.storage().snapshot_exists() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let persisted_blocks = match state.storage().list_blocks() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    let contracts_prepared = state.storage().contract_namespaces_ready();
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let peer_count = state
        .p2p()
        .and_then(|p| p.status().ok())
        .map(|s| s.connected_peers.len())
        .unwrap_or(0);
    let last_block_hash = chain
        .dag
        .blocks
        .values()
        .max_by_key(|b| b.header.height)
        .map(|b| b.hash.clone());

    Json(ApiResponse::ok(NodeStatusData {
        service: "pulsedagd".into(),
        version: repo_version(),
        chain_id: chain.chain_id.clone(),
        best_height: chain.dag.best_height,
        block_count: chain.dag.blocks.len(),
        tip_count: chain.dag.tips.len(),
        mempool_size: chain.mempool.transactions.len(),
        utxo_count: chain.utxo.utxos.len(),
        address_count: chain.utxo.address_index.len(),
        snapshot_exists,
        persisted_block_count: persisted_blocks.len(),
        p2p_enabled: state.p2p().is_some(),
        peer_count,
        last_block_hash,
        contracts_prepared,
        contracts_enabled: chain.contracts.config.enabled,
        contracts_vm_version: chain.contracts.config.vm_version.clone(),
    }))
}
