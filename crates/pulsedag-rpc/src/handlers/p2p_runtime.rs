use axum::{extract::State, Json};
use serde_json::json;

use crate::api::ApiResponse;

pub async fn get_p2p_runtime<S>(State(_state): State<S>) -> Json<ApiResponse<serde_json::Value>> {
    Json(ApiResponse::ok(json!({
        "peer_count": 0,
        "last_peer_message_unix": null,
        "inbound_block_count": 0,
        "inbound_tx_count": 0,
        "outbound_block_count": 0,
        "outbound_tx_count": 0,
        "highest_peer_advertised_height": 0
    })))
}

pub async fn get_sync_lag<S>(State(_state): State<S>) -> Json<ApiResponse<serde_json::Value>> {
    Json(ApiResponse::ok(json!({
        "local_best_height": 0,
        "sync_target_height": 0,
        "sync_lag_blocks": 0,
        "state": "healthy",
        "selected_tip": null
    })))
}
