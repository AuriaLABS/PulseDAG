use crate::api::{ApiResponse, RpcStateLike};
use axum::{extract::State, http::StatusCode, Json};

fn legacy_wallet_rpc_removed() -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    (
        StatusCode::NOT_FOUND,
        Json(ApiResponse::err(
            "legacy_wallet_rpc_removed",
            "raw-key wallet RPC has been removed; sign locally and submit a fully signed transaction",
        )),
    )
}

pub async fn post_wallet_new<S: RpcStateLike>(
    State(_state): State<S>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    legacy_wallet_rpc_removed()
}

pub async fn post_wallet_sign<S: RpcStateLike>(
    State(_state): State<S>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    legacy_wallet_rpc_removed()
}

pub async fn post_wallet_transfer<S: RpcStateLike>(
    State(_state): State<S>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    legacy_wallet_rpc_removed()
}
