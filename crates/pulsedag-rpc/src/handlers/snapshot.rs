use std::time::{SystemTime, UNIX_EPOCH};

use axum::{extract::State, Json};

use crate::{api::ApiResponse, api::RpcStateLike};

#[derive(Debug, serde::Serialize)]
pub struct SnapshotInfoData {
    pub snapshot_exists: bool,
    pub snapshot_height: Option<u64>,
    pub captured_at_unix: Option<u64>,
    pub best_height: u64,
    pub recommended_keep_from_height: u64,
    pub chain_id: Option<String>,
    pub block_count: Option<usize>,
    pub tip_count: Option<usize>,
    pub utxo_count: Option<usize>,
    pub mempool_size: Option<usize>,
    pub persisted_block_count: usize,
    pub startup_path: String,
    pub startup_fastboot_used: bool,
    pub startup_replay_required: bool,
    pub startup_fallback_reason: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct SnapshotCreateData {
    pub snapshot_exists: bool,
    pub snapshot_height: u64,
    pub captured_at_unix: u64,
}

pub async fn get_snapshot_info<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<SnapshotInfoData>> {
    let persisted_blocks = match state.storage().list_blocks() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };
    let persisted_block_count = persisted_blocks.len();

    let chain_handle = state.chain();
    let best_height = chain_handle.read().await.dag.best_height;
    let runtime_handle = state.runtime();
    let runtime = runtime_handle.read().await;
    let keep_recent = runtime.prune_keep_recent_blocks.max(1);
    let startup_path = runtime.startup_path.clone();
    let startup_fastboot_used = runtime.startup_fastboot_used;
    let startup_replay_required = runtime.startup_replay_required;
    let startup_fallback_reason = runtime.startup_fallback_reason.clone();
    let recommended_keep_from_height = best_height.saturating_sub(keep_recent.saturating_sub(1));
    drop(runtime);

    let captured_at_unix = match state.storage().snapshot_captured_at_unix() {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    };

    match state.storage().load_chain_state() {
        Ok(Some(snapshot)) => Json(ApiResponse::ok(SnapshotInfoData {
            snapshot_exists: true,
            snapshot_height: Some(snapshot.dag.best_height),
            captured_at_unix,
            best_height: snapshot.dag.best_height.max(best_height),
            recommended_keep_from_height,
            chain_id: Some(snapshot.chain_id),
            block_count: Some(snapshot.dag.blocks.len()),
            tip_count: Some(snapshot.dag.tips.len()),
            utxo_count: Some(snapshot.utxo.utxos.len()),
            mempool_size: Some(snapshot.mempool.transactions.len()),
            persisted_block_count,
            startup_path: startup_path.clone(),
            startup_fastboot_used,
            startup_replay_required,
            startup_fallback_reason: startup_fallback_reason.clone(),
        })),
        Ok(None) => Json(ApiResponse::ok(SnapshotInfoData {
            snapshot_exists: false,
            snapshot_height: None,
            captured_at_unix,
            best_height,
            recommended_keep_from_height,
            chain_id: None,
            block_count: None,
            tip_count: None,
            utxo_count: None,
            mempool_size: None,
            persisted_block_count,
            startup_path,
            startup_fastboot_used,
            startup_replay_required,
            startup_fallback_reason,
        })),
        Err(e) => Json(ApiResponse::err("STORAGE_ERROR", e.to_string())),
    }
}

pub async fn post_snapshot_create<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<SnapshotCreateData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await.clone();
    if let Err(e) = state.storage().persist_chain_state(&chain) {
        return Json(ApiResponse::err("SNAPSHOT_PERSIST_ERROR", e.to_string()));
    }

    let captured_at_unix = match state.storage().snapshot_captured_at_unix() {
        Ok(Some(v)) => v,
        Ok(None) => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        Err(e) => return Json(ApiResponse::err("SNAPSHOT_METADATA_ERROR", e.to_string())),
    };
    {
        let runtime_handle = state.runtime();
        let mut runtime = runtime_handle.write().await;
        runtime.last_snapshot_height = Some(chain.dag.best_height);
        runtime.last_snapshot_unix = Some(captured_at_unix);
    }
    let _ = state.storage().append_runtime_event(
        "info",
        "snapshot_manual",
        &format!(
            "manual snapshot persisted at height {}",
            chain.dag.best_height
        ),
    );

    Json(ApiResponse::ok(SnapshotCreateData {
        snapshot_exists: true,
        snapshot_height: chain.dag.best_height,
        captured_at_unix,
    }))
}
